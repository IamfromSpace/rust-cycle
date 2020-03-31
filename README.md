# Rust Cycle

Rust Cycle is a (WIP) bicycle computer built on Rust!

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

### Pi Config

#### Enable SPI

SPI is used to interface with the InkyPhat display.

In `/boot/config.txt` add (or uncomment) `dtparam=spi=on`.

Reboot to enable this change.

#### Enable I2C

I2C is used to interface with the button shim.

In `/boot/config.txt` add (or uncomment) `dtparam=i2c_arm=on`.

Reboot to enable this change.

#### Enable GPS

Our GPS module uses UART to send NMEA sentences.
Because we will be using Bluetooth, which uses the primary serial device, we need to use the miniUart for serial communication.

In `/boot/cmdline.txt` remove `console=serial0,115200`.

In `/boot/config.txt` add `enable_uart=1`.

Reboot to enable these changes.

#### Run on Startup

From the host, fill in the `misc/rust-cycle.service` file and send it to the target.

```
scp misc/rust-cycle.service pi@raspberrypi:~/Downloads/
```

Then add the service into the pi's set of services.

```
sudo mv ~/Downloads/rust-cycle.service /etc/systemd/system/
```

On the target, enable the service:

```
sudo systemctl enable rust-cycle.service
```

On reboot, the application will start.
