<p align="center">
  <img src="src-tauri/icons/logo.svg" alt="Switchfetcher logo" width="128" height="128">
</p>

<h1 align="center">Switchfetcher</h1>

<p align="center">
  Desktop account switching and usage tracking for Codex, Claude, and Gemini.
</p>

<p align="center">
  <a href="https://github.com/adxptived/switchfetcher-app/releases"><img alt="Version" src="https://img.shields.io/github/v/release/adxptived/switchfetcher-app"></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2010%2B-0078D6">
</p>>

Switchfetcher keeps multiple local AI accounts in one desktop app, refreshes usage across providers, and lets you switch active credentials without digging through config files by hand.

## Features

- Switch between local Codex accounts with ChatGPT OAuth or imported `auth.json` files.
- Import Claude credentials from `~/.claude/.credentials.json` and refresh usage automatically.
- Add Gemini browser-session accounts for usage monitoring through copied web cookies.
- Refresh usage for all accounts on launch and on demand from the desktop UI.
- Warm up Codex accounts and block unsafe switches while Codex processes are running.
- Export and import accounts through slim text bundles or encrypted backup files.
- Manage notifications, refresh intervals, threshold alerts, and app preferences from Settings.

## Download

Prebuilt binaries are published on GitHub Releases:

- https://github.com/adxptived/switchfetcher-app/releases

Download the latest Windows build and run `Switchfetcher_x64-setup.exe` to install, or use `Switchfetcher.exe` as a portable executable.

## Development Setup

### Prerequisites

- [Node.js](https://nodejs.org/) 18 or newer
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/)

### Install dependencies

```bash
pnpm install
```

### Run in development

```bash
pnpm tauri:dev
```

### Build a production package

```bash
pnpm tauri:build
```

The packaged Windows output is written under `src-tauri/target/release/bundle/`.

## Contributing

Contributions are welcome.

1. Fork the repository.
2. Create a feature branch.
3. Make focused changes with clear commit messages.
4. Run the app locally and verify the affected flows.
5. Open a pull request with a short summary of the change and validation steps.

If you are changing provider integrations or credential handling, include any manual test notes that help reviewers reproduce the behavior safely.

## Storage and Security Notes

- Primary data store: `~/.switchfetcher/accounts.json`
- Codex auth target: `~/.codex/auth.json`
- Claude auth target: `~/.claude/.credentials.json`
- Browser cookies, slim exports, and encrypted backups should be treated as sensitive credentials.

Switchfetcher is intended for managing your own local accounts. It is not built for credential sharing, pooling, or multi-user redistribution.


