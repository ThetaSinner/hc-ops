
### Developer setup

Ubuntu setup:

```bash
apt update
apt install libsqlite3-dev
cargo install diesel_cli --no-default-features --features sqlite

export DATABASE_URL=development.sqlite3
```

In another shell, if you want a Holochain instance to test against:

```bash
nix develop
```

The inside that shell:

```bash
hc s clean && echo "1234" | hc s --piped create && echo "1234" | hc s --piped -f 8888 run
```
