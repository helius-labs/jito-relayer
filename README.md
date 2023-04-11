# jito-relayer
Transaction Relayer

# Setup

```bash
git submodule update --init --recursive
cargo b --release
```

## Generate RSA keys:
One needs to generate RSA keys for JWT key generation and verification. To do that, use the following scripts:
```bash
OUTPUT_DIR=
openssl genrsa --out $OUTPUT_DIR/private.pem
openssl rsa --in $OUTPUT_DIR/private.pem --pubout --out $OUTPUT_DIR/public.pem
```
