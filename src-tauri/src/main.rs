use std::{path::PathBuf, sync::Mutex};

use serde::Serialize;
use tauri::{Manager, State};
use wallet_core::wallet_service::WalletService;
use zeroize::Zeroizing;

struct AppState {
    wallet_service: Mutex<WalletService>,
    store_path: PathBuf,
}

#[derive(Serialize)]
struct WalletSummary {
    id: String,
    name: String,
    created_at: String,
}

impl From<&wallet_core::wallet_store::WalletRecord> for WalletSummary {
    fn from(record: &wallet_core::wallet_store::WalletRecord) -> Self {
        Self {
            id: record.id.clone(),
            name: record.name.clone(),
            created_at: record.created_at.clone(),
        }
    }
}

#[tauri::command]
fn list_wallets(state: State<'_, AppState>) -> Result<Vec<WalletSummary>, String> {
    let service = state
        .wallet_service
        .lock()
        .map_err(|_| "wallet service is unavailable".to_string())?;

    Ok(service
        .list_wallets()
        .iter()
        .map(WalletSummary::from)
        .collect())
}

#[tauri::command]
fn create_wallet(
    name: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("wallet name cannot be empty".to_string());
    }
    if name.chars().count() > 64 {
        return Err("wallet name cannot exceed 64 characters".to_string());
    }
    let password = Zeroizing::new(password);
    if password.is_empty() {
        return Err("password cannot be empty".to_string());
    }
    if password.len() > 1024 {
        return Err("password cannot exceed 1024 bytes".to_string());
    }
    let mut service = state
        .wallet_service
        .lock()
        .map_err(|_| "wallet service is unavailable".to_string())?;
    let record = service.create_and_save_wallet(
        name,
        password.as_str(),
        &state.store_path.to_string_lossy(),
    )?;

    Ok(WalletSummary::from(&record))
}

#[tauri::command]
fn import_wallet(
    name: String,
    mnemonic: String,
    password: String,
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
    let password = Zeroizing::new(password);
    if password.is_empty() {
        return Err("password cannot be empty".to_string());
    }
    if password.len() > 1024 {
        return Err("password cannot exceed 1024 bytes".to_string());
    }
    let mut service = state
        .wallet_service
        .lock()
        .map_err(|_| "wallet service is unavailable".to_string())?;
    let record = service.import_and_save_wallet(
        name,
        mnemonic.as_str(),
        password.as_str(),
        &state.store_path.to_string_lossy(),
    )?;

    Ok(WalletSummary::from(&record))
}

#[tauri::command]
fn reveal_wallet_mnemonic(
    wallet_id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    if password.is_empty() {
        return Err("password cannot be empty".to_string());
    }
    if password.len() > 1024 {
        return Err("password cannot exceed 1024 bytes".to_string());
    }

    let password = Zeroizing::new(password);
    let service = state
        .wallet_service
        .lock()
        .map_err(|_| "wallet service is unavailable".to_string())?;
    let mnemonic = service.reveal_mnemonic(&wallet_id, password.as_str())?;

    Ok(mnemonic.to_string())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let store_path = app_data_dir.join("wallets.json");
            let store_path_string = store_path.to_string_lossy();

            let wallet_service = if store_path.exists() {
                WalletService::load_store(&store_path_string).map_err(std::io::Error::other)?
            } else {
                WalletService::new()
            };

            app.manage(AppState {
                wallet_service: Mutex::new(wallet_service),
                store_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_wallet,
            import_wallet,
            list_wallets,
            reveal_wallet_mnemonic
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HederaVault");
}
