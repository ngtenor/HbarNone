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

The frontend invokes wallet creation, mnemonic import, listing, and password-
gated recovery-phrase reveal commands. Wallet summaries intentionally exclude
mnemonics and private keys. This is early development software; do not use it
to store real funds or import a recovery phrase for real funds.
