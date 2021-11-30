# ws-tool

an easy to use websocket tool.

features:

- support tls & self signed cert
- support setting http proxy & socks5 proxy
- tested by autobahn

## usage

**basic usage**

see `echo` example

### self signed cert

[examples/echo](./examples/echo.rs) show how to connect to a websocket with self signed cert.

to run this example, first running `gen_cert` example to get cert & key for testing

```bash
cargo run --example gen_cert -- wsl.com

-----BEGIN CERTIFICATE-----
MIIBSDCB76ADAgECAgEqMAoGCCqGSM49BAMCMCExHzAdBgNVBAMMFnJjZ2VuIHNl
bGYgc2lnbmVkIGNlcnQwIBcNNzUwMTAxMDAwMDAwWhgPNDA5NjAxMDEwMDAwMDBa
MCExHzAdBgNVBAMMFnJjZ2VuIHNlbGYgc2lnbmVkIGNlcnQwWTATBgcqhkjOPQIB
BggqhkjOPQMBBwNCAAQiCxhMtKyW0sJuBkwSF0nEQ0FYNhkBty08WKlq4wwNAwyb
KkkaRuhI9vFk5P36aPG34t3IDaJLy66xDiQ82szIoxYwFDASBgNVHREECzAJggd3
c2wuY29tMAoGCCqGSM49BAMCA0gAMEUCIFxJTfq0WH3i4MW9O4RNKRxacEArrv3o
HGGhdKaBYHxoAiEAy8WuKhwxNj4rjLT4wzItTTQmfjtajILO1h3qkTvJRrs=
-----END CERTIFICATE-----

-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg8z5eL3nUiyTTL+ll
Jb0YEJ523JthkaQpfF03W/ZhLyGhRANCAAQiCxhMtKyW0sJuBkwSF0nEQ0FYNhkB
ty08WKlq4wwNAwybKkkaRuhI9vFk5P36aPG34t3IDaJLy66xDiQ82szI
-----END PRIVATE KEY-----
```

save cert as target.pem and key as target-key.pem under `scripts` dir.

Then edit your `/etc/hosts`, add dns record for `wsl.com` domain, after that,
run [scripts/ssl_server.py](./scripts/ssl_server.py) to setup a websocket server.

```bash
python scripts/ssl_server.py
running on 0.0.0.0:4430
```

now you can connect websocket server with following command

```bash
cargo run --example echo -- wss://wsl.com:4430 -c ./scripts/target.pem

[SEND] > rookie
[RECV] > Hello rookie!
[SEND] > ^C
```

### use proxy

[examples/binance](./examples/binance.rs) show how to connect via proxy

### run autobaha testsuit

start test server

```bash
./script/start_autobahn_server.sh
```

run test on other terminal

```bash
cargo run --example autobahn-client
```

report files should be under `test_reports` dir.


## autobahn test report

<details>
<summary>click to expand report</summary>

![report](./assets/report.jpeg)
</details>

## TODO

- [ ] add proxy auth config
- [ ] support custom https proxy cert
- [x] split client into writer & reader


## REF

- [WebSocket RFC](https://tools.ietf.org/html/rfc6455)
- [tungstenite-rs](https://github.com/snapview/tungstenite-rs)
