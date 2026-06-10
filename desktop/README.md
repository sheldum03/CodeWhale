# CodeWhale Desktop

Windows desktop workbench for the local CodeWhale runtime.

## Development

Prerequisites:

- Node.js and `npm.cmd`
- Rust toolchain with `cargo` on `PATH`
- WebView2 Runtime
- A working `codewhale.cmd` command for development runtime launch

```powershell
cd desktop
copy .env.example .env
npm.cmd install
npm.cmd run desktop:dev
```

The app reads `desktop/.env` in development and starts:

```powershell
codewhale.cmd serve --http --host 127.0.0.1 --port 7878 --auth-token <token>
```

Do not commit `desktop/.env`; it is ignored by the repository root `.gitignore`.
