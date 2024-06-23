# 💻 Terminal based dashboard with customizable widgets

# Build
## Core(Rust)
### .env
```sh
BIND=0.0.0.0:8282
```

### Linux
```bash
cargo build --release -F ssl
```

### Windows
```bash
cargo build --release
```

## Web Terminal View
```bash
cd front/
yarn
yarn build
touch .env
```

**.env**
```bash
VITE_API=127.0.0.1:8282
```
