#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, sync::Mutex};

use hedera_mirror::{AccountOverview, MirrorClient, TokenInfo};
use hedera_tx::key_tree::KeyTree;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use wallet_core::{
    account_checksum::{parse_account_input, with_checksum},
    key_import::{KeyType, SecretKind, is_valid_ecdsa_secret, parse_private_key, public_key_hex},
    vault_container::Network,
    wallet_service::{
        AddressBookRow, LegacyWalletPassword, MAX_BACKUP_BYTES, SharedAccountSummary,
        WalletService, WalletSummary, validate_account_id,
    },
};
use zeroize::Zeroizing;

struct AppState {
    wallet_service: Mutex<WalletService>,
    store_path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultState {
    status: String,
    wallets: Vec<WalletSummary>,
    legacy_wallets: Vec<WalletSummary>,
    auto_lock_seconds: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyWalletPasswordRequest {
    id: String,
    password: String,
}

fn lock_service(state: &AppState) -> Result<std::sync::MutexGuard<'_, WalletService>, String> {
    state
        .wallet_service
        .lock()
        .map_err(|_| "wallet service is unavailable".to_string())
}

fn unlocked_state(service: &mut WalletService) -> Result<VaultState, String> {
    Ok(VaultState {
        status: service.vault_status().to_string(),
        wallets: service.list_wallets()?,
        legacy_wallets: service.legacy_wallets(),
        auto_lock_seconds: service.auto_lock_seconds(),
    })
}

#[tauri::command]
fn get_vault_state(state: State<'_, AppState>) -> Result<VaultState, String> {
    let mut service = lock_service(&state)?;
    let mut status = service.vault_status().to_string();
    let wallets = if status == "unlocked" {
        match service.list_wallets() {
            Ok(wallets) => wallets,
            Err(error) => {
                status = service.vault_status().to_string();
                if status == "unlocked" {
                    return Err(error);
                }
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    Ok(VaultState {
        status,
        wallets,
        legacy_wallets: service.legacy_wallets(),
        auto_lock_seconds: service.auto_lock_seconds(),
    })
}

#[tauri::command]
fn create_vault(
    vault_password: String,
    auto_lock_seconds: u32,
    state: State<'_, AppState>,
) -> Result<VaultState, String> {
    let password = Zeroizing::new(vault_password);
    let mut service = lock_service(&state)?;
    service.set_auto_lock_seconds(auto_lock_seconds, &state.store_path.to_string_lossy())?;
    service.create_vault(password.as_str(), &state.store_path.to_string_lossy())?;
    unlocked_state(&mut service)
}

#[tauri::command]
fn unlock_vault(vault_password: String, state: State<'_, AppState>) -> Result<VaultState, String> {
    let password = Zeroizing::new(vault_password);
    let mut service = lock_service(&state)?;
    service.unlock_vault(password.as_str())?;
    unlocked_state(&mut service)
}

/// Archives the vault file and returns to the first-run screen, for when the
/// vault password has been lost. The encrypted file is renamed with a
/// `before-reset` suffix, never deleted, so it stays recoverable if the
/// password is found later. No password is needed; anyone with access to the
/// machine could already delete the file, and its contents stay encrypted.
#[tauri::command]
fn start_over_vault(state: State<'_, AppState>) -> Result<VaultState, String> {
    let mut service = lock_service(&state)?;
    service.start_over(&state.store_path.to_string_lossy())?;
    Ok(VaultState {
        status: service.vault_status().to_string(),
        wallets: Vec::new(),
        legacy_wallets: Vec::new(),
        auto_lock_seconds: service.auto_lock_seconds(),
    })
}

#[tauri::command]
fn migrate_legacy_vault(
    vault_password: String,
    legacy_wallet_passwords: Vec<LegacyWalletPasswordRequest>,
    auto_lock_seconds: u32,
    state: State<'_, AppState>,
) -> Result<VaultState, String> {
    let vault_password = Zeroizing::new(vault_password);
    let legacy_wallet_passwords: Vec<_> = legacy_wallet_passwords
        .into_iter()
        .map(|credential| LegacyWalletPassword {
            id: credential.id,
            password: Zeroizing::new(credential.password),
        })
        .collect();
    let mut service = lock_service(&state)?;
    service.migrate_legacy_vault(
        vault_password.as_str(),
        &legacy_wallet_passwords,
        auto_lock_seconds,
        &state.store_path.to_string_lossy(),
    )?;
    unlocked_state(&mut service)
}

#[tauri::command]
fn change_vault_password(
    current_password: String,
    new_password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let current_password = Zeroizing::new(current_password);
    let new_password = Zeroizing::new(new_password);
    lock_service(&state)?.change_vault_password(
        current_password.as_str(),
        new_password.as_str(),
        &state.store_path.to_string_lossy(),
    )
}

#[tauri::command]
async fn export_vault_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let backup = lock_service(&state)?.export_backup()?;
    let Some(selected) = app
        .dialog()
        .file()
        .set_title("Save encrypted vault backup")
        .set_file_name("hbarnone-backup.json")
        .add_filter("HBarNone backup", &["json"])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = selected
        .into_path()
        .map_err(|e| format!("invalid backup location: {e}"))?;
    std::fs::write(&path, backup).map_err(|e| format!("could not write the backup file: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("could not restrict backup file permissions: {e}"))?;
    }
    Ok(true)
}

#[tauri::command]
async fn backup_and_erase_vault(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<VaultState>, String> {
    let backup = lock_service(&state)?.export_backup()?;
    let Some(selected) = app
        .dialog()
        .file()
        .set_title("Save encrypted vault backup before erasing")
        .set_file_name("hbarnone-backup.json")
        .add_filter("HBarNone backup", &["json"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|e| format!("invalid backup location: {e}"))?;
    std::fs::write(&path, &backup)
        .map_err(|e| format!("could not write the backup file: {e}; nothing was erased"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
            format!("could not restrict backup file permissions: {e}; nothing was erased")
        })?;
    }
    // Nothing is erased unless the saved file reads back identical.
    match std::fs::read_to_string(&path) {
        Ok(saved) if saved == backup => {}
        _ => return Err("the saved backup could not be verified; nothing was erased".to_string()),
    }
    let mut service = lock_service(&state)?;
    service.erase_vault(&backup, &state.store_path.to_string_lossy())?;
    Ok(Some(VaultState {
        status: service.vault_status().to_string(),
        wallets: Vec::new(),
        legacy_wallets: Vec::new(),
        auto_lock_seconds: service.auto_lock_seconds(),
    }))
}

#[tauri::command]
async fn restore_vault_backup(
    backup_password: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<VaultState>, String> {
    let backup_password = Zeroizing::new(backup_password);
    let Some(selected) = app
        .dialog()
        .file()
        .set_title("Choose an encrypted vault backup")
        .add_filter("HBarNone backup", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|e| format!("invalid backup location: {e}"))?;
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("could not read the backup file: {e}"))?
        .len();
    if size > MAX_BACKUP_BYTES as u64 {
        return Err("backup file is too large".to_string());
    }
    let contents = std::fs::read_to_string(&path)
        .map_err(|_| "the selected file is not an HBarNone backup".to_string())?;
    let mut service = lock_service(&state)?;
    service.restore_backup(
        &contents,
        backup_password.as_str(),
        &state.store_path.to_string_lossy(),
    )?;
    unlocked_state(&mut service).map(Some)
}

#[tauri::command]
fn set_auto_lock_seconds(seconds: u32, state: State<'_, AppState>) -> Result<u32, String> {
    let mut service = lock_service(&state)?;
    service.set_auto_lock_seconds(seconds, &state.store_path.to_string_lossy())?;
    Ok(service.auto_lock_seconds())
}

#[tauri::command]
fn record_activity(state: State<'_, AppState>) -> Result<(), String> {
    lock_service(&state)?.record_activity()
}

#[tauri::command]
fn lock_vault(state: State<'_, AppState>) -> Result<(), String> {
    lock_service(&state)?.lock();
    Ok(())
}

#[tauri::command]
fn list_wallets(state: State<'_, AppState>) -> Result<Vec<WalletSummary>, String> {
    lock_service(&state)?.list_wallets()
}

#[tauri::command]
fn delete_wallet(wallet_id: String, state: State<'_, AppState>) -> Result<(), String> {
    if wallet_id.trim().is_empty() {
        return Err("wallet ID cannot be empty".to_string());
    }

    lock_service(&state)?.delete_wallet_and_save(&wallet_id, &state.store_path.to_string_lossy())
}

#[tauri::command]
fn link_wallet_account(
    wallet_id: String,
    account_id: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let network = wallet_network(&state, &wallet_id)?;
    let account_id = parse_account_input(&account_id, network, false)?;
    lock_service(&state)?.add_wallet_accounts_and_save(
        &wallet_id,
        network,
        &[account_id],
        &state.store_path.to_string_lossy(),
    )
}

/// The network a wallet is tagged with, or an error if it does not exist.
fn wallet_network(state: &AppState, wallet_id: &str) -> Result<Network, String> {
    lock_service(state)?
        .list_wallets()?
        .into_iter()
        .find(|wallet| wallet.id == wallet_id)
        .map(|wallet| wallet.network)
        .ok_or_else(|| format!("wallet not found: {wallet_id}"))
}

/// The wallet's linked account on the wallet's own network, or an error if it
/// is not one of them.
fn linked_account_of(
    state: &AppState,
    wallet_id: &str,
    account_id: &str,
) -> Result<(String, Network), String> {
    let account_id = validate_account_id(account_id)?;
    let wallet = lock_service(state)?
        .list_wallets()?
        .into_iter()
        .find(|wallet| wallet.id == wallet_id)
        .ok_or_else(|| format!("wallet not found: {wallet_id}"))?;
    let linked = wallet
        .accounts
        .iter()
        .any(|a| a.network == wallet.network && a.id == account_id);
    if linked {
        Ok((account_id, wallet.network))
    } else {
        Err(format!("account {account_id} is not linked to this wallet"))
    }
}

/// The wallet's linked Testnet account, or an error if it is not one of them
/// or the wallet is not a Testnet wallet. Every spending or signing command
/// resolves its payer through this, so Mainnet wallets stay read-only.
fn testnet_account_of(
    state: &AppState,
    wallet_id: &str,
    account_id: &str,
) -> Result<String, String> {
    let (account_id, network) = linked_account_of(state, wallet_id, account_id)?;
    if network != Network::Testnet {
        return Err("this operation is only available for Testnet wallets".to_string());
    }
    Ok(account_id)
}

// Network commands run only when the user clicks or opens a wallet; the
// webview never sees the mirror URL, and only the public key or account ID
// leaves the app.
#[tauri::command]
async fn find_wallet_accounts(
    wallet_id: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let network = wallet_network(&state, &wallet_id)?;
    let public_key = lock_service(&state)?.wallet_public_key(&wallet_id)?;
    let matches = MirrorClient::for_network(network)?
        .find_accounts_by_public_key(&public_key)
        .await?;
    if matches.is_empty() {
        let shown = match network {
            Network::Testnet => "Testnet",
            Network::Mainnet => "Mainnet",
        };
        return Err(format!(
            "no {shown} account uses this wallet's key yet; an account is created when the key's address first receives HBAR"
        ));
    }
    lock_service(&state)?.add_wallet_accounts_and_save(
        &wallet_id,
        network,
        &matches,
        &state.store_path.to_string_lossy(),
    )
}

#[tauri::command]
async fn get_account_overview(
    wallet_id: String,
    account_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AccountOverview>, String> {
    let (account_id, network) = linked_account_of(&state, &wallet_id, &account_id)?;
    MirrorClient::for_network(network)?
        .account_overview(&account_id)
        .await
}

#[tauri::command]
async fn get_token_info(token_id: String) -> Result<Option<TokenInfo>, String> {
    MirrorClient::testnet()?.token_info(&token_id).await
}

/// The key structure of a Testnet account, shown in full before the account
/// is trusted: the threshold, the total and every leaf key. `None` when the
/// account is unknown; an unreadable or unsupported key is an error.
#[derive(Serialize)]
struct AccountKeyStructure {
    /// One line per node, for example `2 of 3:` and one key per line.
    description: String,
    required: u32,
    total: usize,
    /// Lowercase hex leaf keys in display order.
    keys: Vec<String>,
}

#[tauri::command]
async fn get_account_key_structure(
    account_id: String,
) -> Result<Option<AccountKeyStructure>, String> {
    let account_id = parse_account_input(&account_id, Network::Testnet, false)?;
    let Some(raw) = MirrorClient::testnet()?.account_key(&account_id).await? else {
        return Ok(None);
    };
    let tree = hedera_tx::key_tree::KeyTree::from_mirror(&raw.kind, &raw.hex)?;
    let (required, total) = tree.required_and_total();
    Ok(Some(AccountKeyStructure {
        description: tree.describe(),
        required,
        total,
        keys: tree.leaf_keys().into_iter().map(|(_, hex)| hex).collect(),
    }))
}

#[tauri::command]
async fn get_transaction_result(transaction_id: String) -> Result<Option<String>, String> {
    MirrorClient::testnet()?
        .transaction_result(&transaction_id)
        .await
}

#[tauri::command]
async fn get_account_balance(
    wallet_id: String,
    account_id: String,
    state: State<'_, AppState>,
) -> Result<Option<u64>, String> {
    let (account_id, network) = linked_account_of(&state, &wallet_id, &account_id)?;
    MirrorClient::for_network(network)?
        .account_balance(&account_id)
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendResult {
    transaction_id: String,
}

struct DesktopFeeApproval(tauri::AppHandle);

impl hedera_tx::fees::FeeApproval for DesktopFeeApproval {
    async fn approve(&self, current_maximum: u64, required_fee: u64) -> Result<(), String> {
        let app = self.0.clone();
        let message = format!(
            "The node rejected the transaction because its required fee exceeds \
             the estimate plus 25% buffer.\n\n\
             Current maximum: {} HBAR\n\
             Node-quoted fee: {} HBAR\n\n\
             Only the actual network fee is charged, in addition to any transfer \
             or opening balance. Approve this higher maximum and retry once?",
            hedera_tx::fees::format_hbar(current_maximum),
            hedera_tx::fees::format_hbar(required_fee),
        );
        let accepted = tauri::async_runtime::spawn_blocking(move || {
            app.dialog()
                .message(message)
                .title("Network fee exceeds 25% buffer")
                .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancel)
                .blocking_show()
        })
        .await
        .map_err(|e| format!("Could not show network fee approval: {e}"))?;
        if accepted {
            Ok(())
        } else {
            Err("Fee increase declined. The rejected transaction was not resubmitted.".into())
        }
    }
}

/// Creates a fungible token owned by the wallet's key. `initial_supply` is in
/// whole tokens, such as `1000` or `12.5`.
#[tauri::command]
async fn create_token(
    wallet_id: String,
    from_account: String,
    name: String,
    symbol: String,
    decimals: u32,
    initial_supply: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    hedera_tx::check_token_text("name", &name)?;
    hedera_tx::check_token_text("symbol", &symbol)?;
    if decimals > hedera_tx::MAX_TOKEN_DECIMALS {
        return Err("Decimals can be 18 at most.".to_string());
    }
    let supply = hedera_tx::submit::parse_token_amount(&initial_supply, decimals)?;
    let payer = testnet_account_of(&state, &wallet_id, &from_account)?;
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::create_testnet_token(
        &payer,
        (key_type, &secret),
        &name,
        &symbol,
        decimals,
        supply,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Creates an NFT collection owned by the wallet's key. The account is the
/// treasury, and its key is the admin and supply key used to mint serials.
#[tauri::command]
async fn create_nft_collection(
    wallet_id: String,
    from_account: String,
    name: String,
    symbol: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    hedera_tx::check_token_text("name", &name)?;
    hedera_tx::check_token_text("symbol", &symbol)?;
    let payer = testnet_account_of(&state, &wallet_id, &from_account)?;
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::create_testnet_nft_collection(
        &payer,
        (key_type, &secret),
        &name,
        &symbol,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// NFT collections (by ID) this account can mint into: the token is a live
/// NFT, this wallet's key is the supply key and this account is the treasury.
#[tauri::command]
async fn get_nft_supply_tokens(
    wallet_id: String,
    account_id: String,
    token_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let wallet_key = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_ascii_lowercase();
    let mirror = MirrorClient::testnet()?;
    let mut found = Vec::new();
    for token_id in token_ids.iter().take(50) {
        let Ok(token_id) = validate_account_id(token_id) else {
            continue;
        };
        let Ok(Some(info)) = mirror.token_info(&token_id).await else {
            continue;
        };
        if !info.deleted
            && info.token_type == "NON_FUNGIBLE_UNIQUE"
            && info.treasury == account
            && info.supply_key.as_deref() == Some(wallet_key.as_str())
        {
            found.push(token_id);
        }
    }
    Ok(found)
}

/// Mints one new NFT serial number into the treasury account after checking on
/// the mirror node that this wallet's key is the supply key and this account is
/// the treasury, so a doomed transaction never costs a fee.
#[tauri::command]
async fn mint_nft(
    wallet_id: String,
    account_id: String,
    token_id: String,
    metadata: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let payer = testnet_account_of(&state, &wallet_id, &account_id)?;
    let token_id = validate_account_id(&token_id)?;
    let metadata = metadata.trim().to_string();
    if metadata.is_empty() || metadata.len() > hedera_tx::MAX_NFT_METADATA_BYTES {
        return Err("The NFT metadata must be 1 to 100 bytes.".to_string());
    }
    if metadata.chars().any(char::is_control) {
        return Err("The NFT metadata cannot contain control characters.".to_string());
    }
    let mirror = MirrorClient::testnet()?;
    let info = mirror
        .token_info(&token_id)
        .await?
        .ok_or_else(|| "The mirror node does not know this token.".to_string())?;
    if info.deleted {
        return Err("This token has been deleted.".to_string());
    }
    if info.token_type != "NON_FUNGIBLE_UNIQUE" {
        return Err("This token is not an NFT collection.".to_string());
    }
    let wallet_key = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_ascii_lowercase();
    match info.supply_key.as_deref() {
        Some(key) if key == wallet_key => {}
        Some(_) => return Err("This wallet's key is not the collection's supply key.".to_string()),
        None if info.has_supply_key => {
            return Err(
                "This collection's supply key is not one this wallet can sign with.".to_string(),
            );
        }
        None => {
            return Err("This collection has no supply key, so nothing can be minted.".to_string());
        }
    }
    if info.treasury != payer {
        return Err("Only the collection's treasury account can mint here.".to_string());
    }
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::mint_testnet_nft(
        &payer,
        (key_type, &secret),
        &token_id,
        &metadata,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Associates this wallet's account with a fungible token so it can hold it.
#[tauri::command]
async fn associate_token(
    wallet_id: String,
    account_id: String,
    token_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let token_id = validate_account_id(&token_id)?;
    let mirror = MirrorClient::testnet()?;
    check_associate(&mirror, &account, &token_id).await?;
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::change_testnet_token_association(
        &account,
        (key_type, &secret),
        &token_id,
        true,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Removes a token from this wallet's account, after checking that the
/// balance is zero and the account is not the token's treasury.
#[tauri::command]
async fn dissociate_token(
    wallet_id: String,
    account_id: String,
    token_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let token_id = validate_account_id(&token_id)?;
    let mirror = MirrorClient::testnet()?;
    check_dissociate(&mirror, &account, &token_id).await?;
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::change_testnet_token_association(
        &account,
        (key_type, &secret),
        &token_id,
        false,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

#[tauri::command]
fn get_hidden_tokens(wallet_id: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    lock_service(&state)?.hidden_tokens(&wallet_id)
}

#[tauri::command]
fn set_token_hidden(
    wallet_id: String,
    token_id: String,
    hidden: bool,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    lock_service(&state)?.set_token_hidden(
        &wallet_id,
        &token_id,
        hidden,
        &state.store_path.to_string_lossy(),
    )
}

/// Deletes a token after checking on the mirror node that this wallet's key is
/// its admin key, so a doomed transaction never costs a fee.
#[tauri::command]
async fn delete_token(
    wallet_id: String,
    account_id: String,
    token_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let payer = testnet_account_of(&state, &wallet_id, &account_id)?;
    let token_id = validate_account_id(&token_id)?;
    let info = MirrorClient::testnet()?
        .token_info(&token_id)
        .await?
        .ok_or_else(|| "The mirror node does not know this token.".to_string())?;
    if info.deleted {
        return Err("This token has already been deleted.".to_string());
    }
    let wallet_key = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_ascii_lowercase();
    match info.admin_key.as_deref() {
        Some(key) if key == wallet_key => {}
        Some(_) => return Err("This wallet's key is not the token's admin key.".to_string()),
        None if info.has_admin_key => {
            return Err("This token's admin key is not one this wallet can sign with.".to_string());
        }
        None => return Err("This token has no admin key, so it cannot be deleted.".to_string()),
    }
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::delete_testnet_token(
        &payer,
        (key_type, &secret),
        &token_id,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Lists which of `token_ids` this account can mint and burn: fungible, not
/// deleted, this wallet's key is the supply key and this account is the treasury.
#[tauri::command]
async fn get_supply_tokens(
    wallet_id: String,
    account_id: String,
    token_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let wallet_key = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_ascii_lowercase();
    let mirror = MirrorClient::testnet()?;
    let mut found = Vec::new();
    for token_id in token_ids.iter().take(50) {
        let Ok(token_id) = validate_account_id(token_id) else {
            continue;
        };
        let Ok(Some(info)) = mirror.token_info(&token_id).await else {
            continue;
        };
        if !info.deleted
            && info.token_type == "FUNGIBLE_COMMON"
            && info.treasury == account
            && info.supply_key.as_deref() == Some(wallet_key.as_str())
        {
            found.push(token_id);
        }
    }
    Ok(found)
}

/// Mints or burns a token's supply after checking on the mirror node that this
/// wallet's key is the supply key and this account is the treasury, so a doomed
/// transaction never costs a fee. `amount` is in whole tokens, such as `12.5`.
#[tauri::command]
async fn change_token_supply(
    wallet_id: String,
    account_id: String,
    token_id: String,
    amount: String,
    mint: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let payer = testnet_account_of(&state, &wallet_id, &account_id)?;
    let token_id = validate_account_id(&token_id)?;
    let mirror = MirrorClient::testnet()?;
    let info = mirror
        .token_info(&token_id)
        .await?
        .ok_or_else(|| "The mirror node does not know this token.".to_string())?;
    if info.deleted {
        return Err("This token has been deleted.".to_string());
    }
    if info.token_type != "FUNGIBLE_COMMON" {
        return Err("Only fungible tokens are supported for now.".to_string());
    }
    let wallet_key = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_ascii_lowercase();
    match info.supply_key.as_deref() {
        Some(key) if key == wallet_key => {}
        Some(_) => return Err("This wallet's key is not the token's supply key.".to_string()),
        None if info.has_supply_key => {
            return Err(
                "This token's supply key is not one this wallet can sign with.".to_string(),
            );
        }
        None => {
            return Err("This token has no supply key, so its supply cannot change.".to_string());
        }
    }
    if info.treasury != payer {
        return Err("Only the token's treasury account can mint or burn it here.".to_string());
    }
    let units = hedera_tx::submit::parse_token_amount(&amount, info.decimals)?;
    if units == 0 {
        return Err("Amount must be greater than zero.".to_string());
    }
    if !mint {
        let held: u64 = mirror
            .account_overview(&payer)
            .await?
            .and_then(|o| o.tokens.into_iter().find(|t| t.token_id == token_id))
            .map(|t| t.balance.parse().unwrap_or(0))
            .unwrap_or(0);
        if units > held {
            return Err("The treasury balance is too low to burn that amount.".to_string());
        }
    }
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::change_testnet_token_supply(
        &payer,
        (key_type, &secret),
        &token_id,
        units,
        mint,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Signs in the backend with the wallet's own key and submits to Testnet only.
#[tauri::command]
async fn send_hbar(
    wallet_id: String,
    from_account: String,
    recipient: String,
    amount_hbar: String,
    memo: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let tinybars = hedera_tx::submit::parse_hbar(&amount_hbar)?;
    let sender = testnet_account_of(&state, &wallet_id, &from_account)?;
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    ensure_send_allowed(&state, &recipient)?;
    let receipt = hedera_tx::submit::send_testnet_transfer(
        key_type,
        &secret,
        &sender,
        &recipient,
        tinybars,
        &memo,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Sends a fungible token. The mirror node is checked first so a transfer that
/// would fail (token deleted, not enough balance, receiver not associated)
/// never costs a fee. `amount` is in whole tokens, such as `12.5`.
#[tauri::command]
async fn send_token(
    wallet_id: String,
    from_account: String,
    recipient: String,
    token_id: String,
    amount: String,
    memo: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let sender = testnet_account_of(&state, &wallet_id, &from_account)?;
    let token_id = validate_account_id(&token_id)?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    ensure_send_allowed(&state, &recipient)?;
    let mirror = MirrorClient::testnet()?;
    let (units, decimals) =
        check_token_send(&mirror, &sender, &recipient, &token_id, &amount).await?;
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::send_testnet_token(
        (key_type, &secret),
        &sender,
        &recipient,
        &token_id,
        units,
        decimals,
        &memo,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Mirror checks shared by hot and watch-only token sends. Returns the amount
/// in the token's smallest unit and the token's decimals.
async fn check_token_send(
    mirror: &MirrorClient,
    sender: &str,
    recipient: &str,
    token_id: &str,
    amount: &str,
) -> Result<(u64, u32), String> {
    let info = mirror
        .token_info(token_id)
        .await?
        .ok_or_else(|| "The mirror node does not know this token.".to_string())?;
    if info.deleted {
        return Err("This token has been deleted.".to_string());
    }
    if info.token_type != "FUNGIBLE_COMMON" {
        return Err("Only fungible tokens are supported for now.".to_string());
    }
    let units = hedera_tx::submit::parse_token_amount(amount, info.decimals)?;
    if units == 0 {
        return Err("Amount must be greater than zero.".to_string());
    }
    let held = mirror
        .account_overview(sender)
        .await?
        .and_then(|o| o.tokens.into_iter().find(|t| t.token_id == token_id))
        .ok_or_else(|| "This account does not hold that token.".to_string())?;
    let balance: u64 = held
        .balance
        .parse()
        .map_err(|_| "The mirror node returned an unreadable balance.".to_string())?;
    if units > balance {
        return Err("The balance is too low for this amount.".to_string());
    }
    let receiver_associated = mirror
        .account_overview(recipient)
        .await?
        .ok_or_else(|| "The recipient was not found on Testnet.".to_string())?
        .tokens
        .iter()
        .any(|t| t.token_id == token_id);
    if !receiver_associated {
        return Err(
            "The recipient has not associated this token, so it cannot receive it yet.".to_string(),
        );
    }
    Ok((units, info.decimals))
}

/// Parses an NFT serial number such as `7`.
fn parse_serial_number(input: &str) -> Result<i64, String> {
    let text = input.trim();
    if text.is_empty() || text.len() > 19 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Enter the NFT's serial number (a positive whole number).".to_string());
    }
    let serial = text
        .parse::<i64>()
        .map_err(|_| "The serial number is too large.".to_string())?;
    if serial <= 0 {
        return Err("Enter the NFT's serial number (a positive whole number).".to_string());
    }
    Ok(serial)
}

/// Mirror checks shared by hot and watch-only NFT sends: the token is a live
/// NFT, the sender owns that serial number, the receiver is associated, and
/// custom fees are only accepted when Settings allow them.
async fn check_nft_send(
    mirror: &MirrorClient,
    sender: &str,
    recipient: &str,
    token_id: &str,
    serial_number: i64,
    allow_custom_fees: bool,
) -> Result<(), String> {
    let info = mirror
        .token_info(token_id)
        .await?
        .ok_or_else(|| "The mirror node does not know this token.".to_string())?;
    if info.deleted {
        return Err("This token has been deleted.".to_string());
    }
    if info.token_type != "NON_FUNGIBLE_UNIQUE" {
        return Err("This token is not an NFT. Use Send token instead.".to_string());
    }
    if !info.custom_fees.is_empty() && !allow_custom_fees {
        return Err(
            "This NFT's token charges custom fees (for example artist royalties). \
             Sending it is disabled. You can allow such transfers in Settings."
                .to_string(),
        );
    }
    let nft = mirror
        .nft_info(token_id, serial_number)
        .await?
        .ok_or_else(|| "The mirror node does not know that serial number.".to_string())?;
    if nft.deleted {
        return Err("That NFT has been deleted.".to_string());
    }
    if nft.owner != sender {
        return Err("This account does not own that NFT.".to_string());
    }
    let receiver_associated = mirror
        .is_associated(recipient, token_id)
        .await?
        .ok_or_else(|| "The recipient was not found on Testnet.".to_string())?;
    if !receiver_associated {
        return Err(
            "The recipient has not associated this token, so it cannot receive it yet.".to_string(),
        );
    }
    Ok(())
}

/// Sends one NFT by token ID and serial number. The mirror node is checked
/// first so a transfer that would fail (not an NFT, not owned, receiver not
/// associated, custom fees not allowed) never costs a fee.
#[tauri::command]
async fn send_nft(
    wallet_id: String,
    from_account: String,
    recipient: String,
    token_id: String,
    serial_number: String,
    memo: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let sender = testnet_account_of(&state, &wallet_id, &from_account)?;
    let token_id = validate_account_id(&token_id)?;
    let serial = parse_serial_number(&serial_number)?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    ensure_send_allowed(&state, &recipient)?;
    let allow_custom_fees = lock_service(&state)?.allow_nft_custom_fees()?;
    let mirror = MirrorClient::testnet()?;
    check_nft_send(
        &mirror,
        &sender,
        &recipient,
        &token_id,
        serial,
        allow_custom_fees,
    )
    .await?;
    let (key_type, secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::send_testnet_nft(
        (key_type, &secret),
        &sender,
        &recipient,
        &token_id,
        serial,
        &memo,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Mirror checks shared by hot and watch-only associations.
async fn check_associate(
    mirror: &MirrorClient,
    account: &str,
    token_id: &str,
) -> Result<(), String> {
    let info = mirror
        .token_info(token_id)
        .await?
        .ok_or_else(|| "The mirror node does not know this token.".to_string())?;
    if info.deleted {
        return Err("This token has been deleted.".to_string());
    }
    if !matches!(
        info.token_type.as_str(),
        "FUNGIBLE_COMMON" | "NON_FUNGIBLE_UNIQUE"
    ) {
        return Err("Only fungible tokens and NFTs are supported.".to_string());
    }
    if mirror.is_associated(account, token_id).await? == Some(true) {
        return Err("This account is already associated with the token.".to_string());
    }
    Ok(())
}

/// Mirror checks shared by hot and watch-only dissociations.
async fn check_dissociate(
    mirror: &MirrorClient,
    account: &str,
    token_id: &str,
) -> Result<(), String> {
    let info = mirror.token_info(token_id).await?;
    if let Some(info) = &info {
        if info.token_type == "NON_FUNGIBLE_UNIQUE" {
            let associated = mirror
                .is_associated(account, token_id)
                .await?
                .ok_or_else(|| "This account was not found on Testnet.".to_string())?;
            if !associated {
                return Err("This account is not associated with that token.".to_string());
            }
            // NFTs of a deleted token are discarded by the network on dissociation.
            if !info.deleted {
                if !mirror
                    .account_nfts_of_token(account, token_id)
                    .await?
                    .is_empty()
                {
                    return Err(
                        "This account still owns serial numbers of that NFT. Send them away first."
                            .to_string(),
                    );
                }
                if info.treasury == account {
                    return Err("The treasury account cannot remove its own token.".to_string());
                }
            }
            return Ok(());
        }
    }
    let overview = mirror
        .account_overview(account)
        .await?
        .ok_or_else(|| "This account was not found on Testnet.".to_string())?;
    let held = overview
        .tokens
        .iter()
        .find(|t| t.token_id == token_id)
        .ok_or_else(|| "This account is not associated with that token.".to_string())?;
    // A deleted token can be dissociated whatever its balance; the stuck
    // balance is discarded by the network.
    if held.balance != "0" && !held.deleted {
        return Err(
            "The balance must be zero before a token can be removed. Send the rest away first."
                .to_string(),
        );
    }
    if let Some(info) = &info {
        if !info.deleted && info.treasury == account {
            return Err("The treasury account cannot remove its own token.".to_string());
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SigningRequest {
    payload: String,
    qr_size: usize,
    /// One flag per module, row by row; true is dark.
    qr_modules: Vec<bool>,
    /// "transfer", "delete", "create", "token-transfer", "nft-transfer",
    /// "token-associate" or "token-dissociate".
    action: String,
    /// The account that pays and whose key signs.
    sender: String,
    /// The recipient of a transfer or of a deleted account's remaining HBAR.
    receiver: String,
    /// The account a deletion removes.
    delete_account: String,
    /// The public key of a new account, in hex; empty when the key is a
    /// structure (see `key_structure`).
    new_key: String,
    /// The lines of a key structure being set (threshold first, then one key
    /// per line); empty for a single key.
    key_structure: Vec<String>,
    /// The account's current public key in hex, set for a key change so the
    /// UI can show which key must still sign; empty otherwise.
    current_key: String,
    /// HBAR sent, or the opening balance of a new account.
    tinybars: i64,
    /// The token of a token request, or empty.
    token_id: String,
    /// The token amount in its smallest unit, as text.
    token_units: String,
    token_decimals: u32,
    /// The serial number of an NFT transfer, as text; empty otherwise.
    serial_number: String,
    memo: String,
    expires_at_seconds: i64,
}

fn require_watch_only(state: &AppState, wallet_id: &str) -> Result<(), String> {
    let is_watch_only = lock_service(state)?
        .list_wallets()?
        .iter()
        .any(|w| w.id == wallet_id && w.secret_kind == SecretKind::WatchOnly);
    if is_watch_only {
        Ok(())
    } else {
        Err("Signing requests are only for watch-only wallets.".to_string())
    }
}

async fn signing_request(
    request: hedera_tx::offline::UnsignedRequest,
) -> Result<SigningRequest, String> {
    let raw = MirrorClient::testnet()?
        .account_key(&request.description.payer)
        .await?
        .ok_or_else(|| "The fee payer was not found on Testnet.".to_string())?;
    let payer_tree = KeyTree::from_mirror(&raw.kind, &raw.hex)?;
    let mut leaves = payer_tree.leaf_keys();
    if let hedera_tx::offline::Action::KeyUpdate { key } = &request.description.action {
        leaves.extend(key.leaf_keys());
    }
    let keys = leaves
        .into_iter()
        .map(|(kind, hex)| {
            let key = KeyTree::single(kind, &hex)?.to_key()?;
            let bytes = key
                .ed25519
                .or(key.ecdsa_secp256k1)
                .ok_or_else(|| "Unsupported signing key for fee estimation.".to_string())?;
            Ok((kind, bytes))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let (request, _quote) = request.with_estimated_fee(&keys).await?;
    render_signing_request(request)
}

fn render_signing_request(
    request: hedera_tx::offline::UnsignedRequest,
) -> Result<SigningRequest, String> {
    use hedera_tx::offline::Action;
    let code =
        qrcode::QrCode::with_error_correction_level(request.payload.as_bytes(), qrcode::EcLevel::L)
            .map_err(|_| {
                "The request is too large for a QR code. Use a shorter memo.".to_string()
            })?;
    let shown = |id: &str| with_checksum(Network::Testnet, id).unwrap_or_else(|_| id.to_string());
    let d = request.description;
    // A single key shows as hex in `new_key`; a structure as `key_structure` lines.
    let key_display = |key: &hedera_tx::key_tree::KeyTree| match key {
        hedera_tx::key_tree::KeyTree::Single { public_key_hex, .. } => {
            (public_key_hex.clone(), Vec::new())
        }
        tree => (
            String::new(),
            tree.describe().lines().map(str::to_string).collect(),
        ),
    };
    let (action, receiver, delete_account, new_key, key_structure, tinybars) = match &d.action {
        Action::Transfer { receiver, tinybars } => (
            "transfer",
            shown(receiver),
            String::new(),
            String::new(),
            Vec::new(),
            *tinybars,
        ),
        Action::DeleteAccount { account, recipient } => (
            "delete",
            shown(recipient),
            shown(account),
            String::new(),
            Vec::new(),
            0,
        ),
        Action::CreateAccount {
            key,
            initial_tinybars,
        } => {
            let (new_key, key_structure) = key_display(key);
            (
                "create",
                String::new(),
                String::new(),
                new_key,
                key_structure,
                *initial_tinybars,
            )
        }
        Action::TokenTransfer { receiver, .. } => (
            "token-transfer",
            shown(receiver),
            String::new(),
            String::new(),
            Vec::new(),
            0,
        ),
        Action::NftTransfer { receiver, .. } => (
            "nft-transfer",
            shown(receiver),
            String::new(),
            String::new(),
            Vec::new(),
            0,
        ),
        Action::TokenAssociate { .. } => (
            "token-associate",
            String::new(),
            String::new(),
            String::new(),
            Vec::new(),
            0,
        ),
        Action::TokenDissociate { .. } => (
            "token-dissociate",
            String::new(),
            String::new(),
            String::new(),
            Vec::new(),
            0,
        ),
        Action::KeyUpdate { key } => {
            let (new_key, key_structure) = key_display(key);
            (
                "key-update",
                String::new(),
                String::new(),
                new_key,
                key_structure,
                0,
            )
        }
    };
    let (token_id, token_units, token_decimals) = match &d.action {
        Action::TokenTransfer {
            token,
            amount,
            decimals,
            ..
        } => (token.clone(), amount.to_string(), *decimals),
        Action::NftTransfer { token, .. } => (token.clone(), String::new(), 0),
        Action::TokenAssociate { token } | Action::TokenDissociate { token } => {
            (token.clone(), String::new(), 0)
        }
        _ => (String::new(), String::new(), 0),
    };
    let serial_number = match &d.action {
        Action::NftTransfer { serial_number, .. } => serial_number.to_string(),
        _ => String::new(),
    };
    Ok(SigningRequest {
        payload: request.payload,
        qr_size: code.width(),
        qr_modules: code
            .to_colors()
            .into_iter()
            .map(|c| c == qrcode::Color::Dark)
            .collect(),
        action: action.to_string(),
        sender: shown(&d.payer),
        receiver,
        delete_account,
        new_key,
        key_structure,
        current_key: String::new(),
        tinybars,
        token_id,
        token_units,
        token_decimals,
        serial_number,
        memo: d.memo,
        expires_at_seconds: d.expires_at_seconds,
    })
}

/// Builds an unsigned transfer for a watch-only wallet to be signed elsewhere.
#[tauri::command]
async fn prepare_unsigned_transfer(
    wallet_id: String,
    from_account: String,
    recipient: String,
    amount_hbar: String,
    memo: String,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    let tinybars = hedera_tx::submit::parse_hbar(&amount_hbar)?;
    let sender = testnet_account_of(&state, &wallet_id, &from_account)?;
    require_watch_only(&state, &wallet_id)?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    ensure_send_allowed(&state, &recipient)?;
    signing_request(hedera_tx::offline::build_request(
        &sender, &recipient, tinybars, &memo,
    )?)
    .await
}

/// Builds an unsigned token transfer for a watch-only wallet, after the same
/// mirror checks as a hot-wallet send.
#[tauri::command]
async fn prepare_unsigned_token_transfer(
    wallet_id: String,
    from_account: String,
    recipient: String,
    token_id: String,
    amount: String,
    memo: String,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    require_watch_only(&state, &wallet_id)?;
    let sender = testnet_account_of(&state, &wallet_id, &from_account)?;
    let token_id = validate_account_id(&token_id)?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    ensure_send_allowed(&state, &recipient)?;
    let mirror = MirrorClient::testnet()?;
    let (units, decimals) =
        check_token_send(&mirror, &sender, &recipient, &token_id, &amount).await?;
    let units = i64::try_from(units).map_err(|_| "The amount is too large.".to_string())?;
    signing_request(hedera_tx::offline::build_token_transfer_request(
        &sender, &recipient, &token_id, units, decimals, &memo,
    )?)
    .await
}

/// Builds an unsigned NFT transfer for a watch-only wallet, after the same
/// mirror checks as a hot-wallet send.
#[tauri::command]
async fn prepare_unsigned_nft_transfer(
    wallet_id: String,
    from_account: String,
    recipient: String,
    token_id: String,
    serial_number: String,
    memo: String,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    require_watch_only(&state, &wallet_id)?;
    let sender = testnet_account_of(&state, &wallet_id, &from_account)?;
    let token_id = validate_account_id(&token_id)?;
    let serial = parse_serial_number(&serial_number)?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    ensure_send_allowed(&state, &recipient)?;
    let allow_custom_fees = lock_service(&state)?.allow_nft_custom_fees()?;
    let mirror = MirrorClient::testnet()?;
    check_nft_send(
        &mirror,
        &sender,
        &recipient,
        &token_id,
        serial,
        allow_custom_fees,
    )
    .await?;
    signing_request(hedera_tx::offline::build_nft_transfer_request(
        &sender, &recipient, &token_id, serial, &memo,
    )?)
    .await
}

/// Builds an unsigned association or dissociation for a watch-only wallet.
#[tauri::command]
async fn prepare_unsigned_token_association(
    wallet_id: String,
    account_id: String,
    token_id: String,
    associate: bool,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    require_watch_only(&state, &wallet_id)?;
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let token_id = validate_account_id(&token_id)?;
    let mirror = MirrorClient::testnet()?;
    if associate {
        check_associate(&mirror, &account, &token_id).await?;
    } else {
        check_dissociate(&mirror, &account, &token_id).await?;
    }
    signing_request(hedera_tx::offline::build_token_association_request(
        &account, &token_id, associate,
    )?)
    .await
}

/// Builds an unsigned deletion of one of a watch-only wallet's accounts. The
/// remaining HBAR go to another account of the same wallet, so one key signs.
#[tauri::command]
async fn prepare_unsigned_delete(
    wallet_id: String,
    account_id: String,
    recipient: String,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    require_watch_only(&state, &wallet_id)?;
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    let recipient = testnet_account_of(&state, &wallet_id, &recipient)?;
    if recipient == account {
        return Err("Choose a different recipient account.".to_string());
    }
    signing_request(hedera_tx::offline::build_delete_request(
        &account, &recipient,
    )?)
    .await
}

/// Builds an unsigned creation of a new account that uses the watch-only
/// wallet's own key and is funded by one of the wallet's accounts.
#[tauri::command]
async fn prepare_unsigned_create(
    wallet_id: String,
    from_account: String,
    initial_hbar: String,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    require_watch_only(&state, &wallet_id)?;
    let tinybars = hedera_tx::submit::parse_hbar(&initial_hbar)?;
    let payer = testnet_account_of(&state, &wallet_id, &from_account)?;
    let wallet = lock_service(&state)?
        .list_wallets()?
        .into_iter()
        .find(|w| w.id == wallet_id)
        .ok_or_else(|| format!("wallet not found: {wallet_id}"))?;
    let public_key = lock_service(&state)?.wallet_public_key(&wallet_id)?;
    let key = hedera_tx::key_tree::KeyTree::single(wallet.key_type, &public_key)?;
    signing_request(hedera_tx::offline::build_create_request(
        &payer, &key, tinybars, "",
    )?)
    .await
}

/// Verifies the signer's response against the request and submits to Testnet.
#[tauri::command]
async fn submit_signed_request(
    request: String,
    response: String,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    use hedera_tx::offline::Action;
    let verified = hedera_tx::offline::assemble(&request, &response)?;
    if hedera_tx::offline::seconds_remaining(&verified.description)? <= 0 {
        return Err("The signing request has expired. Create a new one.".to_string());
    }
    let payer = verified.description.payer.clone();
    let wallets = lock_service(&state)?.list_wallets()?;
    let wallet = wallets
        .iter()
        .find(|w| {
            w.secret_kind == SecretKind::WatchOnly
                && w.accounts
                    .iter()
                    .any(|a| a.network == Network::Testnet && a.id == payer)
        })
        .ok_or_else(|| "The paying account is not in a watch-only wallet here.".to_string())?;
    let wallet_key = lock_service(&state)?.wallet_public_key(&wallet.id)?;
    if !wallet_key.eq_ignore_ascii_case(&verified.public_key_hex) {
        return Err("The signature was made with a different key than this wallet's.".to_string());
    }
    let owned = |id: &str| {
        wallet
            .accounts
            .iter()
            .any(|a| a.network == Network::Testnet && a.id == id)
    };
    match &verified.description.action {
        Action::Transfer { receiver, .. } | Action::TokenTransfer { receiver, .. } => {
            ensure_send_allowed(&state, receiver)?
        }
        Action::NftTransfer {
            token, receiver, ..
        } => {
            ensure_send_allowed(&state, receiver)?;
            // The Settings policy also covers requests built outside this app.
            ensure_nft_custom_fees_allowed(&state, token).await?;
        }
        Action::TokenAssociate { .. } | Action::TokenDissociate { .. } => {}
        Action::DeleteAccount { account, recipient } => {
            if !owned(account) || !owned(recipient) {
                return Err("Both accounts of a deletion must belong to this wallet.".to_string());
            }
        }
        Action::CreateAccount { key, .. } => {
            let expected = hedera_tx::key_tree::KeyTree::single(wallet.key_type, &wallet_key)?;
            if *key != expected {
                return Err("The new account must use this wallet's own key.".to_string());
            }
        }
        Action::KeyUpdate { .. } => {
            return Err("Use Move account to submit a key change.".to_string());
        }
    }
    let transaction_id = hedera_tx::offline::submit_verified(&verified).await?;
    Ok(SendResult { transaction_id })
}

/// Builds an unsigned key change that moves one of the watch-only wallet's
/// accounts to the target wallet's key. The network requires signatures from
/// both the current key and the new key.
#[tauri::command]
async fn prepare_unsigned_move(
    wallet_id: String,
    account_id: String,
    target_wallet_id: String,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    require_watch_only(&state, &wallet_id)?;
    if target_wallet_id == wallet_id {
        return Err("Choose a different target wallet.".to_string());
    }
    if wallet_network(&state, &target_wallet_id)? != Network::Testnet {
        return Err("The target wallet must be a Testnet wallet.".to_string());
    }
    let target_type = lock_service(&state)?
        .list_wallets()?
        .into_iter()
        .find(|wallet| wallet.id == target_wallet_id)
        .map(|wallet| wallet.key_type)
        .ok_or_else(|| format!("wallet not found: {target_wallet_id}"))?;
    let target_key = lock_service(&state)?.wallet_public_key(&target_wallet_id)?;
    let key = hedera_tx::key_tree::KeyTree::single(target_type, &target_key)?;
    let mut request = signing_request(hedera_tx::offline::build_key_update_request(
        &account, &key,
    )?)
    .await?;
    request.current_key = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_lowercase();
    Ok(request)
}

/// Verifies one scanned signature for a key change and reports which key
/// produced it: "current" for this wallet's key, "new" for the target
/// wallet's key. An invalid signature or any other key is an error.
#[tauri::command]
fn verify_move_signature(
    request: String,
    response: String,
    wallet_id: String,
    target_wallet_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut partial = hedera_tx::offline::begin_collect(&request)?;
    partial.add_response(&response)?;
    let signer = partial
        .signer_keys
        .first()
        .cloned()
        .ok_or_else(|| "This is not a valid signature from a signer.".to_string())?;
    let current = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_lowercase();
    if signer == current {
        return Ok("current".to_string());
    }
    let target = lock_service(&state)?
        .wallet_public_key(&target_wallet_id)?
        .to_lowercase();
    if signer == target {
        return Ok("new".to_string());
    }
    Err("The signature was made with a key that is not part of this move.".to_string())
}

/// Verifies the signed replies for a key change and submits it. The current
/// key's reply is required; the new key's reply is also required unless the
/// target wallet holds its key in the vault, in which case the app co-signs.
#[tauri::command]
async fn submit_signed_move(
    request: String,
    responses: Vec<String>,
    wallet_id: String,
    target_wallet_id: String,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    use hedera_tx::offline::Action;
    let mut partial = hedera_tx::offline::collect_signatures(&request, &responses)?;
    let payer = partial.description.payer.clone();
    testnet_account_of(&state, &wallet_id, &payer)?;
    require_watch_only(&state, &wallet_id)?;
    let old_key = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_lowercase();
    let target_key = lock_service(&state)?
        .wallet_public_key(&target_wallet_id)?
        .to_lowercase();
    let target_type = lock_service(&state)?
        .list_wallets()?
        .into_iter()
        .find(|wallet| wallet.id == target_wallet_id)
        .map(|wallet| wallet.key_type)
        .ok_or_else(|| format!("wallet not found: {target_wallet_id}"))?;
    let expected = hedera_tx::key_tree::KeyTree::single(target_type, &target_key)?;
    match &partial.description.action {
        Action::KeyUpdate { key } if *key == expected => {}
        Action::KeyUpdate { .. } => {
            return Err("The request does not move the account to the chosen wallet.".to_string());
        }
        _ => return Err("This request is not a key change.".to_string()),
    }
    if !partial.signer_keys.contains(&old_key) {
        return Err("The current key's signature is missing.".to_string());
    }
    if !partial.signer_keys.contains(&target_key) {
        let local = lock_service(&state)?.with_wallet_key(&target_wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        });
        match local {
            Ok((key_type, secret)) => partial.sign_locally(key_type, &secret)?,
            Err(_) => {
                return Err("The new key's signature is missing. Sign the request with the target wallet's key too.".to_string());
            }
        }
    }
    let transaction_id = partial.submit().await?;
    Ok(SendResult { transaction_id })
}

/// A shared (multisig) account as shown to the UI: the stored entry plus its
/// parsed key structure.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SharedAccountView {
    id: String,
    display_id: String,
    name: String,
    named: bool,
    /// How many of the listed keys must sign.
    required: u32,
    total: usize,
    /// Lowercase hex leaf keys in display order.
    keys: Vec<String>,
    /// One line per node: the threshold, then one key per line.
    description: String,
}

fn shared_view(summary: SharedAccountSummary) -> Result<SharedAccountView, String> {
    let tree = KeyTree::from_mirror("ProtobufEncoded", &summary.key_hex)?;
    let (required, total) = tree.required_and_total();
    Ok(SharedAccountView {
        id: summary.id,
        display_id: summary.display_id,
        name: summary.name,
        named: summary.named,
        required,
        total,
        keys: tree.leaf_keys().into_iter().map(|(_, hex)| hex).collect(),
        description: tree.describe(),
    })
}

/// The stored key structure of a shared account.
fn shared_tree_of(state: &AppState, account_id: &str) -> Result<KeyTree, String> {
    let key_hex = lock_service(state)?.shared_account_key_hex(Network::Testnet, account_id)?;
    KeyTree::from_mirror("ProtobufEncoded", &key_hex)
}

/// A flat M-of-N structure from pasted or chosen public keys (64 hex
/// characters Ed25519, 66 ECDSA).
fn tree_from_inputs(required: u32, keys: &[String]) -> Result<KeyTree, String> {
    let leaves = keys
        .iter()
        .map(|key| {
            let hex = key.trim().to_ascii_lowercase();
            let key_type = match hex.len() {
                64 => KeyType::Ed25519,
                66 => KeyType::Ecdsa,
                _ => {
                    return Err(
                        "Each key must be 64 (Ed25519) or 66 (ECDSA) hexadecimal characters."
                            .to_string(),
                    );
                }
            };
            KeyTree::single(key_type, &hex)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if leaves.len() < 2 {
        return Err("A shared account needs at least two keys.".to_string());
    }
    let tree = KeyTree::Threshold {
        required,
        keys: leaves,
    };
    tree.validate()?;
    Ok(tree)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SharedSignatureProgress {
    /// Lowercase hex keys that have signed, in the order collected.
    signed_keys: Vec<String>,
    required: u32,
    total: usize,
    /// How many of the structure's keys have signed.
    matched: usize,
    satisfied: bool,
}

/// Progress of the collected signatures against the account's structure. A
/// verified signature from a key outside the structure is an error, so an
/// unrelated reply is caught the moment it is added.
fn shared_progress(
    partial: &hedera_tx::offline::PartiallySigned,
    tree: &KeyTree,
) -> Result<SharedSignatureProgress, String> {
    for key in &partial.signer_keys {
        if !tree.contains_key(key) {
            return Err(format!(
                "A signature was made with a key that is not part of this account: {key}"
            ));
        }
    }
    let (required, total) = tree.required_and_total();
    Ok(SharedSignatureProgress {
        signed_keys: partial.signer_keys.clone(),
        required,
        total,
        matched: total - partial.missing_keys(tree).len(),
        satisfied: partial.satisfies(tree),
    })
}

/// Fetches the account's key structure from the mirror node and stores the
/// account as a shared (multisig) account.
async fn link_shared_account(
    state: &AppState,
    account_id: &str,
    name: Option<String>,
) -> Result<SharedAccountView, String> {
    let raw = MirrorClient::testnet()?
        .account_key(account_id)
        .await?
        .ok_or_else(|| "The account is unknown on Testnet.".to_string())?;
    let tree = KeyTree::from_mirror(&raw.kind, &raw.hex)?;
    if tree.required_and_total().1 < 2 {
        return Err(
            "This account uses a single key. Link it to a wallet on the home screen instead."
                .to_string(),
        );
    }
    let summary = lock_service(state)?.add_shared_account_and_save(
        Network::Testnet,
        account_id,
        name.as_deref().map(str::trim).filter(|n| !n.is_empty()),
        &tree.to_hex()?,
        &state.store_path.to_string_lossy(),
    )?;
    shared_view(summary)
}

/// Every shared (multisig) account the vault watches.
#[tauri::command]
fn list_shared_accounts(state: State<'_, AppState>) -> Result<Vec<SharedAccountView>, String> {
    lock_service(&state)?
        .list_shared_accounts()?
        .into_iter()
        .map(shared_view)
        .collect()
}

/// Links an existing Testnet account whose key is a structure. The structure
/// is fetched from the mirror node and stored for display and for checking
/// collected signatures.
#[tauri::command]
async fn add_shared_account(
    account_id: String,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<SharedAccountView, String> {
    let account_id = parse_account_input(&account_id, Network::Testnet, false)?;
    link_shared_account(&state, &account_id, name).await
}

/// Stops watching a shared account. Nothing changes on the network.
#[tauri::command]
fn remove_shared_account(account_id: String, state: State<'_, AppState>) -> Result<(), String> {
    lock_service(&state)?.remove_shared_account_and_save(
        Network::Testnet,
        &account_id,
        &state.store_path.to_string_lossy(),
    )
}

/// Balance and recent transactions of a shared account.
#[tauri::command]
async fn get_shared_overview(
    account_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AccountOverview>, String> {
    let account_id = validate_account_id(&account_id)?;
    lock_service(&state)?.shared_account_key_hex(Network::Testnet, &account_id)?;
    MirrorClient::testnet()?.account_overview(&account_id).await
}

/// Creates a new Testnet account controlled by an M-of-N key structure,
/// funded by a hot-wallet account that signs and pays. The new account shows
/// up on the mirror node once consensus is reached (`finish_shared_create`).
#[tauri::command]
async fn create_shared_account(
    from_account: String,
    initial_hbar: String,
    required: u32,
    keys: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let tinybars = hedera_tx::submit::parse_hbar(&initial_hbar)?;
    let payer = parse_account_input(&from_account, Network::Testnet, false)?;
    let tree = tree_from_inputs(required, &keys)?;
    let payer_wallet_id = lock_service(&state)?
        .list_wallets()?
        .into_iter()
        .find(|wallet| {
            wallet
                .accounts
                .iter()
                .any(|a| a.network == Network::Testnet && a.id == payer)
        })
        .map(|wallet| wallet.id)
        .ok_or_else(|| "The funding account must belong to a wallet in this vault.".to_string())?;
    let (payer_type, payer_secret) = lock_service(&state)?
        .with_wallet_key(&payer_wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::create_testnet_shared_account(
        &tree,
        &payer,
        (payer_type, &payer_secret),
        tinybars,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Links the account a `create_shared_account` transaction created, once the
/// mirror node shows it.
#[tauri::command]
async fn finish_shared_create(
    transaction_id: String,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<SharedAccountView, String> {
    let (result, entity) = MirrorClient::testnet()?
        .transaction_outcome(&transaction_id)
        .await?
        .ok_or_else(|| "Testnet has not confirmed the creation yet.".to_string())?;
    if result != "SUCCESS" {
        let detail = if result == "INSUFFICIENT_TX_FEE" {
            " The estimated fee plus 25% was insufficient. No account was created; \
             network fees may still have been charged. Obtain a fresh estimate before retrying."
        } else {
            ""
        };
        return Err(format!(
            "The account creation failed: {result}.{detail} Transaction: {transaction_id}"
        ));
    }
    let account_id =
        entity.ok_or_else(|| "Testnet has not confirmed the creation yet.".to_string())?;
    link_shared_account(&state, &account_id, name).await
}

/// Builds an unsigned HBAR transfer from a shared account. Each participant
/// signs the same request; the app merges the replies and submits.
#[tauri::command]
async fn prepare_shared_transfer(
    account_id: String,
    recipient: String,
    amount_hbar: String,
    memo: String,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    let tinybars = hedera_tx::submit::parse_hbar(&amount_hbar)?;
    let sender = validate_account_id(&account_id)?;
    lock_service(&state)?.shared_account_key_hex(Network::Testnet, &sender)?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    ensure_send_allowed(&state, &recipient)?;
    signing_request(hedera_tx::offline::build_request(
        &sender, &recipient, tinybars, &memo,
    )?)
    .await
}

/// Verifies the collected replies against the request and reports progress
/// towards the account's threshold. Duplicate signatures are kept once;
/// invalid, mismatched or unrelated ones are errors.
#[tauri::command]
fn check_shared_signatures(
    request: String,
    responses: Vec<String>,
    state: State<'_, AppState>,
) -> Result<SharedSignatureProgress, String> {
    let mut partial = hedera_tx::offline::begin_collect(&request)?;
    for response in &responses {
        partial.add_response(response)?;
    }
    let tree = shared_tree_of(&state, &partial.description.payer.clone())?;
    shared_progress(&partial, &tree)
}

/// Submits a shared-account HBAR transfer once the collected signatures
/// satisfy the account's threshold. A doomed transaction is never submitted.
#[tauri::command]
async fn submit_shared_transfer(
    request: String,
    responses: Vec<String>,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    use hedera_tx::offline::Action;
    let partial = hedera_tx::offline::collect_signatures(&request, &responses)?;
    let payer = partial.description.payer.clone();
    let tree = shared_tree_of(&state, &payer)?;
    match &partial.description.action {
        Action::Transfer { receiver, .. } => ensure_send_allowed(&state, receiver)?,
        _ => return Err("This request is not an HBAR transfer.".to_string()),
    }
    let progress = shared_progress(&partial, &tree)?;
    if !progress.satisfied {
        return Err(format!(
            "Only {} of the required {} keys have signed. Collect the missing signatures within the time limit.",
            progress.matched, progress.required
        ));
    }
    let transaction_id = partial.submit().await?;
    Ok(SendResult { transaction_id })
}

/// Builds an unsigned key change that puts one of the wallet's accounts under
/// an M-of-N key structure. The current key must sign, plus enough of the new
/// keys to satisfy the threshold.
#[tauri::command]
async fn prepare_shared_key_change(
    wallet_id: String,
    account_id: String,
    required: u32,
    keys: Vec<String>,
    state: State<'_, AppState>,
) -> Result<SigningRequest, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let tree = tree_from_inputs(required, &keys)?;
    signing_request(hedera_tx::offline::build_key_update_request(
        &account, &tree,
    )?)
    .await
}

/// Verifies the replies for a key change to a structure and submits it. The
/// vault co-signs with the current key and any participant keys it holds;
/// the rest must come as signer replies.
#[tauri::command]
async fn submit_shared_key_change(
    wallet_id: String,
    request: String,
    responses: Vec<String>,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    use hedera_tx::offline::Action;
    let mut partial = hedera_tx::offline::begin_collect(&request)?;
    for response in &responses {
        partial.add_response(response)?;
    }
    let payer = partial.description.payer.clone();
    testnet_account_of(&state, &wallet_id, &payer)?;
    let tree = match &partial.description.action {
        Action::KeyUpdate {
            key: key @ KeyTree::Threshold { .. },
        } => key.clone(),
        Action::KeyUpdate { .. } => {
            return Err("Use Move account for a change to a single key.".to_string());
        }
        _ => return Err("This request is not a key change.".to_string()),
    };
    let old_key = lock_service(&state)?
        .wallet_public_key(&wallet_id)?
        .to_lowercase();
    // The vault co-signs with the current key and any participant keys it holds.
    if !partial.signer_keys.contains(&old_key) {
        let local = lock_service(&state)?.with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        });
        match local {
            Ok((key_type, secret)) => partial.sign_locally(key_type, &secret)?,
            Err(_) => return Err("The current key's signature is missing.".to_string()),
        }
    }
    for wallet in lock_service(&state)?.list_wallets()? {
        if wallet.secret_kind == SecretKind::WatchOnly {
            continue;
        }
        let Ok(key_hex) = lock_service(&state)?.wallet_public_key(&wallet.id) else {
            continue;
        };
        let key_hex = key_hex.to_lowercase();
        if tree.contains_key(&key_hex) && !partial.signer_keys.contains(&key_hex) {
            let local = lock_service(&state)?.with_wallet_key(&wallet.id, |key_type, secret| {
                Ok((key_type, Zeroizing::new(*secret)))
            });
            if let Ok((key_type, secret)) = local {
                partial.sign_locally(key_type, &secret)?;
            }
        }
    }
    for key in &partial.signer_keys {
        if *key != old_key && !tree.contains_key(key) {
            return Err(format!(
                "A signature was made with a key that is neither the current key nor part of the new structure: {key}"
            ));
        }
    }
    if !partial.satisfies(&tree) {
        let (required, _) = tree.required_and_total();
        return Err(format!(
            "The new structure needs {required} of its keys to sign. Collect the missing signatures within the time limit."
        ));
    }
    let transaction_id = partial.submit().await?;
    Ok(SendResult { transaction_id })
}

/// Moves the account from its wallet to the shared-accounts list, but only
/// once Testnet shows a key structure on it.
#[tauri::command]
async fn finish_shared_convert(
    wallet_id: String,
    account_id: String,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<SharedAccountView, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let raw = MirrorClient::testnet()?
        .account_key(&account)
        .await?
        .ok_or_else(|| "The account is unknown on Testnet.".to_string())?;
    let tree = KeyTree::from_mirror(&raw.kind, &raw.hex)?;
    if tree.required_and_total().1 < 2 {
        return Err("Testnet has not confirmed the key change yet.".to_string());
    }
    let summary = lock_service(&state)?.convert_account_to_shared_and_save(
        &wallet_id,
        Network::Testnet,
        &account,
        name.as_deref().map(str::trim).filter(|n| !n.is_empty()),
        &tree.to_hex()?,
        &state.store_path.to_string_lossy(),
    )?;
    shared_view(summary)
}

/// Creates an account controlled by this wallet's key. The funding account must
/// belong to a wallet in the vault, whose key signs and pays.
#[tauri::command]
async fn create_account(
    wallet_id: String,
    from_account: String,
    initial_hbar: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let tinybars = hedera_tx::submit::parse_hbar(&initial_hbar)?;
    let payer = parse_account_input(&from_account, Network::Testnet, false)?;
    let payer_wallet_id = lock_service(&state)?
        .list_wallets()?
        .into_iter()
        .find(|wallet| {
            wallet
                .accounts
                .iter()
                .any(|a| a.network == Network::Testnet && a.id == payer)
        })
        .map(|wallet| wallet.id)
        .ok_or_else(|| "The funding account must belong to a wallet in this vault.".to_string())?;
    let (new_type, new_secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let (payer_type, payer_secret) = lock_service(&state)?
        .with_wallet_key(&payer_wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::create_testnet_account(
        (new_type, &new_secret),
        &payer,
        (payer_type, &payer_secret),
        tinybars,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Submits a key change for one of the wallet's Testnet accounts, moving it to
/// the key of another wallet in this vault. Both keys sign. The link between
/// wallets only moves later, in `finish_account_move`, once Testnet confirms.
#[tauri::command]
async fn move_account(
    wallet_id: String,
    account_id: String,
    target_wallet_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    if wallet_id == target_wallet_id {
        return Err("Choose a different wallet.".to_string());
    }
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let (current_type, current_secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let (new_type, new_secret) = lock_service(&state)?
        .with_wallet_key(&target_wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let receipt = hedera_tx::submit::change_testnet_account_key(
        &account,
        (current_type, &current_secret),
        (new_type, &new_secret),
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Moves the account's link to the target wallet, but only if Testnet now
/// shows the target wallet's key on the account.
#[tauri::command]
async fn finish_account_move(
    wallet_id: String,
    account_id: String,
    target_wallet_id: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let target_key = lock_service(&state)?.wallet_public_key(&target_wallet_id)?;
    let owned = MirrorClient::testnet()?
        .find_accounts_by_public_key(&target_key)
        .await?;
    if !owned.contains(&account) {
        return Err("Testnet has not confirmed the key change yet.".to_string());
    }
    lock_service(&state)?.move_wallet_account_and_save(
        &wallet_id,
        &target_wallet_id,
        &account,
        &state.store_path.to_string_lossy(),
    )
}

/// Deletes one of the wallet's Testnet accounts and sends its remaining HBAR
/// to `recipient`. The deleted account's key signs; if the recipient is also in
/// this vault, its key signs too (the same key signs only once).
#[tauri::command]
async fn delete_account(
    wallet_id: String,
    account_id: String,
    recipient: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SendResult, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    let recipient = parse_account_input(&recipient, Network::Testnet, true)?;
    ensure_delete_recipient_allowed(&state, &recipient)?;
    if recipient == account {
        return Err("Choose a different recipient account.".to_string());
    }
    let recipient_wallet = lock_service(&state)?
        .list_wallets()?
        .into_iter()
        .find(|w| {
            w.accounts
                .iter()
                .any(|a| a.network == Network::Testnet && a.id == recipient)
        })
        .map(|w| w.id);
    let (own_type, own_secret) = lock_service(&state)?
        .with_wallet_key(&wallet_id, |key_type, secret| {
            Ok((key_type, Zeroizing::new(*secret)))
        })?;
    let other = match &recipient_wallet {
        Some(id) if *id != wallet_id => Some(
            lock_service(&state)?.with_wallet_key(id, |key_type, secret| {
                Ok((key_type, Zeroizing::new(*secret)))
            })?,
        ),
        _ => None,
    };
    let mut signers: Vec<(KeyType, &[u8; 32])> = vec![(own_type, &own_secret)];
    if let Some((kind, secret)) = &other {
        signers.push((*kind, secret));
    }
    let receipt = hedera_tx::submit::delete_testnet_account(
        &account,
        &recipient,
        recipient_wallet.is_some(),
        &signers,
        &DesktopFeeApproval(app),
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Removes the account's link, but only if Testnet reports it deleted.
#[tauri::command]
async fn finish_account_delete(
    wallet_id: String,
    account_id: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let account = testnet_account_of(&state, &wallet_id, &account_id)?;
    if !MirrorClient::testnet()?
        .account_is_deleted(&account)
        .await?
    {
        return Err("Testnet has not confirmed the deletion yet.".to_string());
    }
    lock_service(&state)?.remove_wallet_account_and_save(
        &wallet_id,
        Network::Testnet,
        &account,
        &state.store_path.to_string_lossy(),
    )
}

/// Checks a typed destination account (checksum required). A send or a delete
/// also enforces the matching address-book limit.
#[tauri::command]
fn check_recipient(
    recipient: String,
    for_send: bool,
    for_delete: Option<bool>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let id = parse_account_input(&recipient, Network::Testnet, true)?;
    if for_send {
        ensure_send_allowed(&state, &id)?;
    }
    if for_delete.unwrap_or(false) {
        ensure_delete_recipient_allowed(&state, &id)?;
    }
    Ok(id)
}

fn ensure_delete_recipient_allowed(state: &AppState, recipient: &str) -> Result<(), String> {
    let mut service = lock_service(state)?;
    if service.allow_external_deletes()?
        || service.address_book_contains(Network::Testnet, recipient)?
    {
        Ok(())
    } else {
        Err(
            "Funds from a deleted account can only go to accounts in the address book. \
             Remove this limit in Settings."
                .to_string(),
        )
    }
}

fn ensure_send_allowed(state: &AppState, recipient: &str) -> Result<(), String> {
    let mut service = lock_service(state)?;
    if service.allow_external_sends()?
        || service.address_book_contains(Network::Testnet, recipient)?
    {
        Ok(())
    } else {
        Err(
            "Send is limited to accounts in the address book. Remove this limit in Settings."
                .to_string(),
        )
    }
}

/// Rejects an NFT transfer whose token carries custom fees (or whose fee
/// schedule cannot be read) unless Settings allow such transfers.
async fn ensure_nft_custom_fees_allowed(state: &AppState, token_id: &str) -> Result<(), String> {
    if lock_service(state)?.allow_nft_custom_fees()? {
        return Ok(());
    }
    let has_fees = MirrorClient::testnet()?
        .token_info(token_id)
        .await?
        .is_none_or(|info| !info.custom_fees.is_empty());
    if has_fees {
        Err(
            "This NFT's token charges custom fees (for example artist royalties). \
             Sending it is disabled. You can allow such transfers in Settings."
                .to_string(),
        )
    } else {
        Ok(())
    }
}

#[tauri::command]
fn get_allow_nft_custom_fees(state: State<'_, AppState>) -> Result<bool, String> {
    lock_service(&state)?.allow_nft_custom_fees()
}

#[tauri::command]
fn set_allow_nft_custom_fees(allowed: bool, state: State<'_, AppState>) -> Result<bool, String> {
    lock_service(&state)?
        .set_allow_nft_custom_fees(allowed, &state.store_path.to_string_lossy())?;
    lock_service(&state)?.allow_nft_custom_fees()
}

#[tauri::command]
fn rename_account(
    wallet_id: String,
    account_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let network = wallet_network(&state, &wallet_id)?;
    lock_service(&state)?.rename_wallet_account_and_save(
        &wallet_id,
        network,
        &account_id,
        &name,
        &state.store_path.to_string_lossy(),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddressBookView {
    rows: Vec<AddressBookRow>,
    allow_external_sends: bool,
    allow_external_deletes: bool,
}

fn address_book_view(state: &AppState) -> Result<AddressBookView, String> {
    let mut service = lock_service(state)?;
    Ok(AddressBookView {
        rows: service.address_book()?,
        allow_external_sends: service.allow_external_sends()?,
        allow_external_deletes: service.allow_external_deletes()?,
    })
}

#[tauri::command]
fn get_address_book(state: State<'_, AppState>) -> Result<AddressBookView, String> {
    address_book_view(&state)
}

/// Removes address-book rows whose accounts no longer exist on their network
/// (deleted, or wiped entirely as after a Testnet reset): outside entries
/// leave the book, and wallet-linked accounts are unlinked from their
/// wallets. A mirror error keeps a row, so a network problem never empties
/// the book.
#[tauri::command]
async fn prune_address_book(state: State<'_, AppState>) -> Result<AddressBookView, String> {
    let entries = lock_service(&state)?.outside_entries()?;
    let mut dead = Vec::new();
    for (network, id) in entries {
        if let Ok(true) = MirrorClient::for_network(network)?
            .account_is_gone(&id)
            .await
        {
            dead.push((network, id));
        }
    }
    // Wallet-linked accounts gone from their network are unlinked from their
    // wallets, so the wallets and the home list stay honest too.
    let wallets = lock_service(&state)?.list_wallets()?;
    let mut dead_links = Vec::new();
    for wallet in wallets {
        for account in &wallet.accounts {
            if account.network != wallet.network {
                continue;
            }
            if let Ok(true) = MirrorClient::for_network(wallet.network)?
                .account_is_gone(&account.id)
                .await
            {
                dead_links.push((wallet.id.clone(), wallet.network, account.id.clone()));
            }
        }
    }
    if !dead.is_empty() || !dead_links.is_empty() {
        let mut service = lock_service(&state)?;
        let path = state.store_path.to_string_lossy();
        for (network, id) in &dead {
            service.remove_address_book_entry(*network, id, &path)?;
        }
        for (wallet_id, network, id) in &dead_links {
            service.remove_wallet_account_and_save(wallet_id, *network, id, &path)?;
        }
    }
    address_book_view(&state)
}

#[tauri::command]
fn set_allow_external_deletes(
    allowed: bool,
    state: State<'_, AppState>,
) -> Result<AddressBookView, String> {
    lock_service(&state)?
        .set_allow_external_deletes(allowed, &state.store_path.to_string_lossy())?;
    address_book_view(&state)
}

#[tauri::command]
fn set_allow_external_sends(
    allowed: bool,
    state: State<'_, AppState>,
) -> Result<AddressBookView, String> {
    lock_service(&state)?.set_allow_external_sends(allowed, &state.store_path.to_string_lossy())?;
    address_book_view(&state)
}

/// Adds a third-party recipient. The checksum is required, and the account
/// must exist on Testnet, so a mistyped address is caught now.
#[tauri::command]
async fn add_address_book_entry(
    name: String,
    address: String,
    state: State<'_, AppState>,
) -> Result<AddressBookView, String> {
    let id = parse_account_input(&address, Network::Testnet, true)?;
    wallet_core::wallet_service::validate_account_name(&name)?;
    if MirrorClient::testnet()?
        .account_balance(&id)
        .await?
        .is_none()
    {
        return Err(format!("Account {id} was not found on Hedera Testnet."));
    }
    lock_service(&state)?.add_address_book_entry(
        Network::Testnet,
        &id,
        &name,
        &state.store_path.to_string_lossy(),
    )?;
    address_book_view(&state)
}

#[tauri::command]
fn rename_address_book_account(
    account_id: String,
    network: Network,
    name: String,
    state: State<'_, AppState>,
) -> Result<AddressBookView, String> {
    lock_service(&state)?.rename_address_book_account(
        network,
        &account_id,
        &name,
        &state.store_path.to_string_lossy(),
    )?;
    address_book_view(&state)
}

#[tauri::command]
fn remove_address_book_entry(
    account_id: String,
    network: Network,
    state: State<'_, AppState>,
) -> Result<AddressBookView, String> {
    lock_service(&state)?.remove_address_book_entry(
        network,
        &account_id,
        &state.store_path.to_string_lossy(),
    )?;
    address_book_view(&state)
}

/// Balance of any account in the address book (wallet account or outside entry).
#[tauri::command]
async fn get_address_balance(
    account_id: String,
    network: Network,
    state: State<'_, AppState>,
) -> Result<Option<u64>, String> {
    let id = validate_account_id(&account_id)?;
    if !lock_service(&state)?.address_book_contains(network, &id)? {
        return Err(format!("account {id} is not in the address book"));
    }
    MirrorClient::for_network(network)?
        .account_balance(&id)
        .await
}

#[tauri::command]
fn rename_wallet(
    wallet_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    if wallet_id.trim().is_empty() {
        return Err("wallet ID cannot be empty".to_string());
    }
    lock_service(&state)?.rename_wallet_and_save(
        &wallet_id,
        &name,
        &state.store_path.to_string_lossy(),
    )
}

#[tauri::command]
fn create_wallet(name: String, state: State<'_, AppState>) -> Result<WalletSummary, String> {
    lock_service(&state)?.create_and_save_wallet(&name, &state.store_path.to_string_lossy())
}

#[tauri::command]
fn import_wallet(
    name: String,
    mnemonic: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let mnemonic = Zeroizing::new(mnemonic);
    if mnemonic.is_empty() {
        return Err("recovery phrase cannot be empty".to_string());
    }
    if mnemonic.len() > 1024 {
        return Err("recovery phrase cannot exceed 1024 bytes".to_string());
    }
    lock_service(&state)?.import_and_save_wallet(
        &name,
        mnemonic.as_str(),
        &state.store_path.to_string_lossy(),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyDetection {
    key_type: Option<KeyType>,
    accounts: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WalletKeys {
    public_key: String,
    evm_address: Option<String>,
}

// A DER key states its own curve. A bare hex key does not, so both candidate
// public keys are looked up on Testnet; the curve is decided only if exactly
// one of them owns an account. Only public keys leave the app.
#[tauri::command]
async fn detect_private_key_type(private_key: String) -> Result<KeyDetection, String> {
    let private_key = Zeroizing::new(private_key);
    if private_key.len() > 1024 {
        return Err("private key cannot exceed 1024 bytes".to_string());
    }
    let parsed = parse_private_key(private_key.as_str())?;
    if let Some(key_type) = parsed.key_type {
        return Ok(KeyDetection {
            key_type: Some(key_type),
            accounts: Vec::new(),
        });
    }
    let client = MirrorClient::testnet()?;
    let ed_accounts = client
        .find_accounts_by_public_key(&public_key_hex(KeyType::Ed25519, &parsed.secret)?)
        .await?;
    let ecdsa_accounts = if is_valid_ecdsa_secret(&parsed.secret) {
        client
            .find_accounts_by_public_key(&public_key_hex(KeyType::Ecdsa, &parsed.secret)?)
            .await?
    } else {
        Vec::new()
    };
    Ok(match (ed_accounts.is_empty(), ecdsa_accounts.is_empty()) {
        (false, true) => KeyDetection {
            key_type: Some(KeyType::Ed25519),
            accounts: ed_accounts,
        },
        (true, false) => KeyDetection {
            key_type: Some(KeyType::Ecdsa),
            accounts: ecdsa_accounts,
        },
        _ => KeyDetection {
            key_type: None,
            accounts: Vec::new(),
        },
    })
}

#[tauri::command]
fn import_private_key(
    name: String,
    private_key: String,
    key_type: Option<KeyType>,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let private_key = Zeroizing::new(private_key);
    lock_service(&state)?.import_private_key_and_save(
        &name,
        private_key.as_str(),
        key_type,
        &state.store_path.to_string_lossy(),
    )
}

#[tauri::command]
fn import_watch_only_wallet(
    name: String,
    public_key: String,
    key_type: KeyType,
    network: Network,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    if public_key.len() > 200 {
        return Err("public key cannot exceed 200 characters".to_string());
    }
    lock_service(&state)?.import_watch_only_and_save(
        &name,
        &public_key,
        key_type,
        network,
        &state.store_path.to_string_lossy(),
    )
}

#[tauri::command]
fn get_wallet_keys(wallet_id: String, state: State<'_, AppState>) -> Result<WalletKeys, String> {
    let mut service = lock_service(&state)?;
    Ok(WalletKeys {
        public_key: service.wallet_public_key(&wallet_id)?,
        evm_address: service.wallet_evm_address(&wallet_id)?,
    })
}

#[tauri::command]
fn reveal_wallet_mnemonic(wallet_id: String, state: State<'_, AppState>) -> Result<String, String> {
    Ok(lock_service(&state)?
        .reveal_mnemonic(&wallet_id)?
        .to_string())
}

#[tauri::command]
fn reveal_wallet_private_key(
    wallet_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    Ok(lock_service(&state)?
        .reveal_derived_private_key(&wallet_id)?
        .to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&app_data_dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let store_path = app_data_dir.join("wallets.json");
            let wallet_service = WalletService::load_store(&store_path.to_string_lossy())
                .map_err(std::io::Error::other)?;

            app.manage(AppState {
                wallet_service: Mutex::new(wallet_service),
                store_path,
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let state = window.state::<AppState>();
                if let Ok(mut service) = state.wallet_service.lock() {
                    service.lock();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            create_vault,
            unlock_vault,
            start_over_vault,
            migrate_legacy_vault,
            set_auto_lock_seconds,
            change_vault_password,
            export_vault_backup,
            backup_and_erase_vault,
            restore_vault_backup,
            record_activity,
            lock_vault,
            create_wallet,
            delete_wallet,
            rename_wallet,
            link_wallet_account,
            prepare_unsigned_transfer,
            prepare_unsigned_delete,
            prepare_unsigned_create,
            prepare_unsigned_token_transfer,
            prepare_unsigned_nft_transfer,
            prepare_unsigned_token_association,
            prepare_unsigned_move,
            submit_signed_request,
            submit_signed_move,
            verify_move_signature,
            find_wallet_accounts,
            get_account_balance,
            get_token_info,
            get_account_key_structure,
            create_token,
            create_nft_collection,
            mint_nft,
            get_nft_supply_tokens,
            delete_token,
            change_token_supply,
            get_supply_tokens,
            associate_token,
            send_token,
            send_nft,
            dissociate_token,
            get_hidden_tokens,
            set_token_hidden,
            get_allow_nft_custom_fees,
            set_allow_nft_custom_fees,
            get_transaction_result,
            get_account_overview,
            list_shared_accounts,
            add_shared_account,
            remove_shared_account,
            get_shared_overview,
            create_shared_account,
            finish_shared_create,
            prepare_shared_transfer,
            check_shared_signatures,
            submit_shared_transfer,
            prepare_shared_key_change,
            submit_shared_key_change,
            finish_shared_convert,
            send_hbar,
            create_account,
            move_account,
            finish_account_move,
            delete_account,
            finish_account_delete,
            check_recipient,
            rename_account,
            get_address_book,
            prune_address_book,
            set_allow_external_sends,
            set_allow_external_deletes,
            add_address_book_entry,
            remove_address_book_entry,
            rename_address_book_account,
            get_address_balance,
            import_wallet,
            import_private_key,
            import_watch_only_wallet,
            detect_private_key_type,
            get_wallet_keys,
            list_wallets,
            reveal_wallet_mnemonic,
            reveal_wallet_private_key,
            get_vault_state
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HBarNone");
}
