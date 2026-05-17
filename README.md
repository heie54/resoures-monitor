# Resource Monitor - Windows Desktop Application

A Windows desktop application built with Tauri + Vue 3 for monitoring system resources (CPU, memory, disk usage).

## Prerequisites

Before running this project, ensure you have the following installed:

### Required Software

| Software | Version | Purpose |
|----------|---------|---------|
| **Node.js** | v18+ | JavaScript runtime for frontend build |
| **npm** | v9+ | Package manager (comes with Node.js) |
| **Rust** | 1.70+ | Rust programming language |
| **Cargo** | (comes with Rust) | Rust package manager |

### Optional: Tauri CLI

For development, you can use the Tauri CLI directly:

```powershell
# Install Tauri CLI globally
npm install -g @tauri-apps/cli

# Or use it via npx (no install required)
npx tauri dev
```

## Installation

### 1. Clone and Install Dependencies

```powershell
# Navigate to project directory
cd D:\rust_program\resource-monitor

# Install npm dependencies
npm install
```

### 2. Verify Rust Installation

```powershell
# Check Rust version
rustc --version
cargo --version
```

## Running in Development Mode

### Option A: Using npm scripts (recommended)

```powershell
# Run Vue frontend in dev mode
npm run dev

# In a separate terminal, run Tauri dev
npm run tauri dev
```

### Option B: Using Tauri CLI directly

```powershell
# Start the complete Tauri development server
npx tauri dev
```

### Option C: Using Vite + Tauri concurrently

```powershell
# Terminal 1: Start Vue dev server
npm run dev

# Terminal 2: Start Tauri
npx tauri dev
```

## Building for Windows

### Development Build (faster, not optimized)

```powershell
npm run tauri build
```

### Production Build

```powershell
# Full production build
npm run build
npm run tauri build
```

The executable will be located at:
```
src-tauri/target/release/resource-monitor.exe
```

## Project Structure

```
resource-monitor/
├── src/                    # Vue frontend source
│   ├── components/         # Vue components
│   ├── App.vue             # Main Vue application
│   └── main.js             # Vue entry point
├── src-tauri/              # Rust/Tauri backend
│   ├── src/
│   │   └── main.rs         # Rust entry point
│   ├── Cargo.toml          # Rust dependencies
│   ├── tauri.conf.json     # Tauri configuration
│   └── build.rs            # Build script
├── public/                 # Static assets
├── scripts/
│   └── build.ps1          # Windows build script
├── package.json            # Node.js dependencies
├── vite.config.js         # Vite configuration
└── README.md              # This file
```

## Dependencies Explanation

### Frontend (Node.js)

| Package | Purpose |
|---------|---------|
| **vue** | Vue 3 reactive framework |
| **@vitejs/plugin-vue** | Vite plugin for Vue SFC support |
| **vite** | Next-generation frontend build tool |

### Backend (Rust)

| Crate | Purpose |
|-------|---------|
| **tauri** | Desktop app framework |
| **serde** | Serialization/deserialization |
| **serde_json** | JSON support |
| **sysinfo** | System information (CPU, memory, disk) |
| **tokio** | Async runtime |

## Troubleshooting

### Rust Issues

```powershell
# Update Rust to latest version
rustup update

# Verify Rust is properly installed
rustc --version
```

### Node.js Issues

```powershell
# Clear npm cache if you have issues
npm cache clean --force

# Reinstall dependencies
rm -rf node_modules
npm install
```

### Tauri Build Issues

```powershell
# Install required Windows build tools
rustup target add x86_64-pc-windows-msvc

# If you see linking errors, install Visual Studio Build Tools
# or ensure you have the Windows SDK installed
```

## Additional Resources

- [Tauri Documentation](https://tauri.app/v1/guides/)
- [Vue 3 Documentation](https://vuejs.org/guide/)
- [Vite Documentation](https://vitejs.dev/guide/)