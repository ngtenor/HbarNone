#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, sync::Mutex};

use hedera_mirror::{AccountOverview, MirrorClient};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use wallet_core::{
    key_import::{KeyType, is_valid_ecdsa_secret, parse_private_key, public_key_hex},
    wallet_service::{LegacyWalletPassword, MAX_BACKUP_BYTES, WalletService, WalletSummary},
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
    account_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    lock_service(&state)?.set_wallet_account_id_and_save(
        &wallet_id,
        account_id.as_deref(),
        &state.store_path.to_string_lossy(),
    )
}

// Network commands run only when the user clicks; the webview never sees the
// mirror URL, and only the public key or account ID leaves the app.
#[tauri::command]
async fn find_wallet_account(
    wallet_id: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let public_key = lock_service(&state)?.wallet_public_key(&wallet_id)?;
    let matches = MirrorClient::testnet()?
        .find_accounts_by_public_key(&public_key)
        .await?;
    match matches.as_slice() {
        [] => Err("no Testnet account uses this wallet's key yet; an account is created when the key's address first receives HBAR".to_string()),
        [account_id] => lock_service(&state)?.set_wallet_account_id_and_save(
            &wallet_id,
            Some(account_id),
            &state.store_path.to_string_lossy(),
        ),
        _ => Err("several Testnet accounts use this key; link the one you want by its account ID".to_string()),
    }
}

#[tauri::command]
async fn get_account_overview(
    wallet_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AccountOverview>, String> {
    let account_id = lock_service(&state)?
        .list_wallets()?
        .into_iter()
        .find(|wallet| wallet.id == wallet_id)
        .ok_or_else(|| format!("wallet not found: {wallet_id}"))?
        .account_id
        .ok_or_else(|| "link a Hedera account to this wallet first".to_string())?;
    MirrorClient::testnet()?.account_overview(&account_id).await
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
            find_wallet_account,
            get_account_overview,
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
