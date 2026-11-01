#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, sync::Mutex};

use hedera_mirror::{AccountOverview, MirrorClient};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use wallet_core::{
    account_checksum::parse_account_input,
    key_import::{KeyType, is_valid_ecdsa_secret, parse_private_key, public_key_hex},
    vault_container::Network,
    wallet_service::{
        AddressBookRow, LegacyWalletPassword, MAX_BACKUP_BYTES, WalletService, WalletSummary,
        validate_account_id,
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
        .set_file_name("hedera-vault-backup.json")
        .add_filter("HederaVault backup", &["json"])
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
        .set_file_name("hedera-vault-backup.json")
        .add_filter("HederaVault backup", &["json"])
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
        .add_filter("HederaVault backup", &["json"])
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
        .map_err(|_| "the selected file is not a HederaVault backup".to_string())?;
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
    let account_id = parse_account_input(&account_id, Network::Testnet, false)?;
    lock_service(&state)?.add_wallet_accounts_and_save(
        &wallet_id,
        Network::Testnet,
        &[account_id],
        &state.store_path.to_string_lossy(),
    )
}

/// The wallet's linked Testnet account, or an error if it is not one of them.
fn testnet_account_of(
    state: &AppState,
    wallet_id: &str,
    account_id: &str,
) -> Result<String, String> {
    let account_id = validate_account_id(account_id)?;
    let linked = lock_service(state)?
        .list_wallets()?
        .into_iter()
        .find(|wallet| wallet.id == wallet_id)
        .ok_or_else(|| format!("wallet not found: {wallet_id}"))?
        .accounts
        .iter()
        .any(|a| a.network == Network::Testnet && a.id == account_id);
    if linked {
        Ok(account_id)
    } else {
        Err(format!("account {account_id} is not linked to this wallet"))
    }
}

// Network commands run only when the user clicks or opens a wallet; the
// webview never sees the mirror URL, and only the public key or account ID
// leaves the app.
#[tauri::command]
async fn find_wallet_accounts(
    wallet_id: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let public_key = lock_service(&state)?.wallet_public_key(&wallet_id)?;
    let matches = MirrorClient::testnet()?
        .find_accounts_by_public_key(&public_key)
        .await?;
    if matches.is_empty() {
        return Err("no Testnet account uses this wallet's key yet; an account is created when the key's address first receives HBAR".to_string());
    }
    lock_service(&state)?.add_wallet_accounts_and_save(
        &wallet_id,
        Network::Testnet,
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
    let account_id = testnet_account_of(&state, &wallet_id, &account_id)?;
    MirrorClient::testnet()?.account_overview(&account_id).await
}

#[tauri::command]
async fn get_account_balance(
    wallet_id: String,
    account_id: String,
    state: State<'_, AppState>,
) -> Result<Option<u64>, String> {
    let account_id = testnet_account_of(&state, &wallet_id, &account_id)?;
    MirrorClient::testnet()?.account_balance(&account_id).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendResult {
    transaction_id: String,
}

/// Signs in the backend with the wallet's own key and submits to Testnet only.
#[tauri::command]
async fn send_hbar(
    wallet_id: String,
    from_account: String,
    recipient: String,
    amount_hbar: String,
    memo: String,
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
        key_type, &secret, &sender, &recipient, tinybars, &memo,
    )
    .await?;
    Ok(SendResult {
        transaction_id: receipt.transaction_id,
    })
}

/// Creates an account controlled by this wallet's key. The funding account must
/// belong to a wallet in the vault, whose key signs and pays.
#[tauri::command]
async fn create_account(
    wallet_id: String,
    from_account: String,
    initial_hbar: String,
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

#[tauri::command]
fn rename_account(
    wallet_id: String,
    account_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    lock_service(&state)?.rename_wallet_account_and_save(
        &wallet_id,
        Network::Testnet,
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
    name: String,
    state: State<'_, AppState>,
) -> Result<AddressBookView, String> {
    lock_service(&state)?.rename_address_book_account(
        Network::Testnet,
        &account_id,
        &name,
        &state.store_path.to_string_lossy(),
    )?;
    address_book_view(&state)
}

#[tauri::command]
fn remove_address_book_entry(
    account_id: String,
    state: State<'_, AppState>,
) -> Result<AddressBookView, String> {
    lock_service(&state)?.remove_address_book_entry(
        Network::Testnet,
        &account_id,
        &state.store_path.to_string_lossy(),
    )?;
    address_book_view(&state)
}

/// Balance of any account in the address book (wallet account or outside entry).
#[tauri::command]
async fn get_address_balance(
    account_id: String,
    state: State<'_, AppState>,
) -> Result<Option<u64>, String> {
    let id = validate_account_id(&account_id)?;
    if !lock_service(&state)?.address_book_contains(Network::Testnet, &id)? {
        return Err(format!("account {id} is not in the address book"));
    }
    MirrorClient::testnet()?.account_balance(&id).await
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
            find_wallet_accounts,
            get_account_balance,
            get_account_overview,
            send_hbar,
            create_account,
            move_account,
            finish_account_move,
            delete_account,
            finish_account_delete,
            check_recipient,
            rename_account,
            get_address_book,
            set_allow_external_sends,
            set_allow_external_deletes,
            add_address_book_entry,
            remove_address_book_entry,
            rename_address_book_account,
            get_address_balance,
            import_wallet,
            import_private_key,
            detect_private_key_type,
            get_wallet_keys,
            list_wallets,
            reveal_wallet_mnemonic,
            get_vault_state
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HederaVault");
}
