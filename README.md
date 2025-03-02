
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

### Using in a test setup

With a running conductor, tag the conductor:

```bash
cargo run --features discover -- conductor-tag add test
```

Build the fixture:

```bash
cd fixture
./package.sh
cd ..
```

Install the fixture:

```bash
cargo run -- admin --tag test install-app ./fixture/happ/fixture.happ
```

Initialise the fixture:

```bash
cargo run -- init --tag test execute fixture
```

When prompted for hte zome to call, use `fixture`.

You should see `Init result: Pass`.
