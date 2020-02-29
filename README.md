# Rust Cycle

## Build

### For Pi Zero

#### Install libs

Install the rustup target:

```
rustup target add arm-unknown-linux-gnueabihf
```

Get the linker from raspberrypi tools.

Point cargo to the linker in `.cargo/config`.

#### Create Binary

create the executable:

```
cargo build --release --target=arm-unknown-linux-gnueabihf
```

#### Deploy

On the host or target (where ever its running from) add the appropriate network capabilities.
This has to be done on every new build!

```
sudo setcap 'cap_net_raw,cap_net_admin+eip' ${BINARY}
```
