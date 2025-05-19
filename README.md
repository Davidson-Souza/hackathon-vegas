# running


Before you can run this, you'll need to install [`phoenixd`](https://github.com/ACINQ/phoenixd/), a lightweight lightning wallet by acinq.

Once everything is set up, open the datadir of your `phoenixd` instance and find the `phonix.conf` file. This should contain two passwords: `http-password` and `http-password-limited`. Copy the `http-password-limited` password and export as an environment variable:

```bash
export PASSWORD=<your_password>
```

Then, run the following command to start the server:

```bash
cargo run --release
```

The api will be available at `http://localhost:8080.
