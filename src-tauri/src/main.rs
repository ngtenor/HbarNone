use std::{path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use wallet_core::wallet_service::{LegacyWalletPassword, WalletService, WalletSummary};
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
fn create_wallet(name: String, state: State<'_, AppState>) -> Result<WalletSummary, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("wallet name cannot be empty".to_string());
    }
    if name.chars().count() > 64 {
        return Err("wallet name cannot exceed 64 characters".to_string());
    }
    lock_service(&state)?.create_and_save_wallet(name, &state.store_path.to_string_lossy())
}

#[tauri::command]
fn import_wallet(
    name: String,
    mnemonic: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("wallet name cannot be empty".to_string());
    }
    if name.chars().count() > 64 {
        return Err("wallet name cannot exceed 64 characters".to_string());
    }
    let mnemonic = Zeroizing::new(mnemonic);
    if mnemonic.is_empty() {
        return Err("recovery phrase cannot be empty".to_string());
    }
    if mnemonic.len() > 1024 {
        return Err("recovery phrase cannot exceed 1024 bytes".to_string());
    }
    lock_service(&state)?.import_and_save_wallet(
        name,
        mnemonic.as_str(),
        &state.store_path.to_string_lossy(),
    )
}

#[tauri::command]
fn reveal_wallet_mnemonic(wallet_id: String, state: State<'_, AppState>) -> Result<String, String> {
    Ok(lock_service(&state)?
        .reveal_mnemonic(&wallet_id)?
        .to_string())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
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
            record_activity,
            lock_vault,
            create_wallet,
            delete_wallet,
            import_wallet,
            list_wallets,
            reveal_wallet_mnemonic,
            get_vault_state
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HederaVault");
}
