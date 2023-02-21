# Contributing

## How to contribute

### By reporting bugs

If you find a bug, please report it to the [issue tracker](https://github.com/SpoticordMusic/spoticord) on GitHub. When reporting bugs, it is recommended that you make use of the provided template.

When filing your bug, please be as precise as possible. Bugs that can be reproduced by anyone are much easier to fix. If you can, please include a minimal test case that demonstrates the bug. This makes it much easier to track down the bug.

### By suggesting new features

If you have an idea for a new feature, please suggest it by creating a new issue on the [issue tracker][issues] on GitHub. When suggesting new features, it is recommended that you make use of the provided template. If you think that your feature is related to an existing issue, please mention it in your description.

If you want to suggest new features more casually, rather than officially here on GitHub, you can join our [Discord server](https://discord.gg/wRCyhVqBZ5) and discuss your ideas with us there.

### By writing code

If you want to contribute code, you can do so through GitHub by forking the repository and sending a pull request.

It is generally recommended that you create an issue on the [issue tracker](https://github.com/SpoticordMusic/spoticord) on GitHub before you start working on a feature. This allows us to discuss the feature and make sure that it is something that we want to add to the project. If you are not sure whether a feature is something that we want to add, you can always ask us on our [Discord server](https://discord.gg/wRCyhVqBZ5).

The flow will look something like this:

1. Fork the repository on GitHub
2. Create a named feature branch (like `add_component_x`)
3. Write your change
4. Test your change
5. Submit a Pull Request to the dev branch

A member of the team will review your pull request and either merge it, request changes to it, or close it with an explanation.

### Code style

When writing code we ask you to code in a way that is consistent with the rest of the codebase. This means that you should use the same indentation style, naming conventions, etc. as the rest of the codebase.

We make use of `rustfmt` to format our code. You can install it by running `rustup component add rustfmt` and then running `cargo fmt --all` to format your code. It is generally recommended to run this command before you commit your code. If you use VSCode, you can install the [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=matklad.rust-analyzer) extension and enable the `Format on Save` option.

#### Git Hooks

We make use of the pre-commit git hook to run `rustfmt` and `clippy` before you commit your code. To set up the git hooks you can run the following command:

  ```bash
  git config core.hooksPath .githooks
  ```

If you want to skip this check, you can use the `--no-verify` flag in your git commit command. Do note however that code that does not pass these checks will not be merged.