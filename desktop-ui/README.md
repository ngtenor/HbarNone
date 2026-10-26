# HederaVault Desktop UI

This React + TypeScript frontend is served by Vite and embedded in the Tauri 2
desktop application.

From the repository root:

```powershell
npm install
Set-Location desktop-ui
npm install
Set-Location ..
npm run tauri:dev
```

Build the frontend independently with:

```powershell
Set-Location desktop-ui
npm run build
```

The frontend invokes only the wallet creation and listing commands. Backend
responses intentionally exclude mnemonics and private keys. This is early
development software; do not use it to store real funds or recovery phrases.
