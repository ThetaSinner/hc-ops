
### Developer setup

Ubuntu setup:

```bash
apt update
apt install libsqlite3-dev
cargo install diesel_cli --no-default-features --features sqlite

export DATABASE_URL=development.sqlite3
```
