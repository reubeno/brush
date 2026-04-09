# Brush Test Images

mkosi configs for building minimal Fedora images with brush as `/bin/sh` and the default login shell.

## Prerequisites

Build brush first:

```bash
cargo build --release
```

## Container (systemd-nspawn)

```bash
mkosi --profile=container build
sudo mkosi --profile=container boot
```

## VM (KVM-accelerated)

```bash
mkosi --profile=vm build
mkosi --profile=vm vm
```

## Options

Use a debug build:

```bash
mkosi --environment=BRUSH_PROFILE=debug --profile=container build
```

Force rebuild:

```bash
mkosi -f --profile=container build
```
