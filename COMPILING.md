# Compiling from source

## Initial setup

Spoticord is built using [rust](https://www.rust-lang.org/), so you'll need to install that first. It is cross-platform, so it should work on Windows, Linux and MacOS. You can find more info about how to install rust [here](https://www.rust-lang.org/tools/install).

### Rust formatter

Spoticord uses [rustfmt](https://github.com/rust-lang/rustfmt) to format the code, and we ask everyone that contributes to Spoticord to use it as well. You can install it by running the following command in your terminal:

```sh
rustup component add rustfmt
```

If you are using VSCode, you can install the [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=matklad.rust-analyzer) extension, which will automatically format your code when you save it (if you have `format on save` enabled). Although rust-analyzer is recommended anyway, as it provides a lot of useful features.

## Build dependencies

On Windows you'll need to install the [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2019) to be able to compile executables in rust (this will also be explained during the rust installation).

If you are on Linux, you can use your package manager to install the following dependencies:

```sh
# Debian/Ubuntu
sudo apt install build-essential

# Arch
sudo pacman -S base-devel

# Fedora
sudo dnf install gcc
```

Additionally, you will need to install CMake. On Windows, you can download CMake [here](https://cmake.org/download/). On Linux, you can use your package manager to install it:

```sh
# Debian/Ubuntu
sudo apt install cmake

# Arch
sudo pacman -S cmake

# Fedora
sudo dnf install cmake
```

## Compiling

Now that you have all the dependencies installed, you can compile Spoticord. To do this, you'll first need to clone the repository:

```sh
git clone https://github.com/SpoticordMusic/spoticord.git
```

After cloning the repo run the following command in the root of the repository:

```sh
cargo build
```

Or if you want to build a release version:

```sh
cargo build --release
```

This will compile the bot and place the executable in `target/release`. You can now run the bot by running the following command:

```sh
./target/release/spoticord
```

If you are actively developing Spoticord, you can use the following command to build and run the bot (this is easier than building and running the bot manually):

```sh
cargo run
```

# Features

As of now, Spoticord has one optional feature: `stats`. This feature enables collecting a few statistics, total and active servers. These statistics will be sent to a redis server, where they then can be read for whatever purpose. If you want to enable this feature, you can do so by running the following command:

```sh
cargo build [--release] --features stats
```

# MSRV

The current minimum supported rust version is `1.75.0` _(Checked with `cargo-msrv`)_.
