Proof-of-concept of using an HSM to generate and store key pairs, then using those key pairs to create a CA certificate, client certificate and server certificate for TLS.

The PKCS#11 bindings and openssl engine from this PoC were incorporated into https://github.com/Azure/iot-identity-service for hardware key storage, so this repo has been archived.

# Pre-requisites

- Linux
- The `openssl` library
- `softhsm`, or any other PKCS#11 library.


# How to run

1. Build the project

    ```sh
    cargo build
    ```

1. If using softhsm, clean all existing softhsm slots.

    ```sh
    rm -rf ~/softhsm &&
    mkdir ~/softhsm
    ```

    where `~/softhsm` is the value of `directories.tokendir` in `/etc/softhsm2.conf`

1. Set env vars for the PKCS#11 library path, and for the PKCS#11 Spy path if you want to use it.

    ```sh
    export PKCS11_LIB_PATH='/usr/lib64/softhsm/libsofthsm.so'
    export PKCS11_SPY_PATH='/usr/lib64/pkcs11/pkcs11-spy.so' # Optional
    ```

1. Initialize three slots.

    If you already have an initialized slot in your HSM, set:

    - `TOKEN` to the token label of the slot
    - `USER_PIN` to the user PIN of the slot
    - `LABEL_{1,2,3}` to the values of the object labels that will be used for the three generated key pairs.

    ```sh
    TOKEN='Key pairs'
    USER_PIN='1234'

    LABEL_1='CA'

    LABEL_2='Server'

    LABEL_3='Client'
    ```

    Otherwise, initialize them here:

    ```sh
    SO_PIN="so$USER_PIN"
    ```

    - For softhsm, use `softhsm2-util` or `pkcs11-tool`. Eg:

        ```sh
        softhsm2-util --init-token --free --label "$TOKEN" --so-pin "$SO_PIN" --pin "$USER_PIN"
        ```

    - For TPM 2.0 TPMs, use `tpm2_ptool` or any other tool that uses TSS. Eg:

        ```sh
        tpm2_ptool addtoken --pobj-pin 'dummy' --pid 1 --label "$TOKEN" --sopin "$SO_PIN" --userpin "$USER_PIN"
        ```

1. Generate a key pair in each of the two slots.

    ```sh
    KEY_1_TYPE='ec-p256'

    KEY_2_TYPE='ec-p256'

    KEY_3_TYPE='ec-p256'

    cargo run -- generate-key-pair --key "pkcs11:token=$TOKEN;object=$LABEL_1?pin-value=$USER_PIN" --type "$KEY_1_TYPE"

    cargo run -- generate-key-pair --key "pkcs11:token=$TOKEN;object=$LABEL_2?pin-value=$USER_PIN" --type "$KEY_2_TYPE"

    cargo run -- generate-key-pair --key "pkcs11:token=$TOKEN;object=$LABEL_3?pin-value=$USER_PIN" --type "$KEY_3_TYPE"
    ```

    Possible values for `--type` are listed in the output of `cargo run -- generate-key-pair --help`

    Each invocation of `generate-key-pair` will print the public key parameters of the newly generated key - modulus and exponent for RSA, curve name and point for EC.

1. Verify the key pairs.

    ```sh
    cargo run -- load --keys "pkcs11:token=$TOKEN;object=$LABEL_1" "pkcs11:token=$TOKEN;object=$LABEL_2" "pkcs11:token=$TOKEN;object=$LABEL_3"
    ```

    This should print the same key parameters that `generate-key-pair` invocations in the previous step did.

1. Generate certificates using the key pairs

    This uses the first key pair to generate a CA cert (self-signed), the second key pair to generate a server cert (signed by the CA cert), and the third key pair to generate a client cert (also signed by the CA cert).

    ```sh
    cargo run -- generate-ca-cert \
        --key "pkcs11:token=$TOKEN;object=$LABEL_1?pin-value=$USER_PIN" \
        --subject 'CA Inc' \
        --out-file "$PWD/ca.pem"

    cargo run -- generate-server-cert \
        --key "pkcs11:token=$TOKEN;object=$LABEL_2?pin-value=$USER_PIN" \
        --subject 'Server LLC' \
        --ca-cert "$PWD/ca.pem" --ca-key "pkcs11:token=$TOKEN;object=$LABEL_1?pin-value=$USER_PIN" \
        --out-file "$PWD/server.pem"

    cargo run -- generate-client-cert \
        --key "pkcs11:token=$TOKEN;object=$LABEL_3?pin-value=$USER_PIN" \
        --subject 'Client GmbH' \
        --ca-cert "$PWD/ca.pem" --ca-key "pkcs11:token=$TOKEN;object=$LABEL_1?pin-value=$USER_PIN" \
        --out-file "$PWD/client.pem"
    ```

1. Start a webserver using the server cert.

    ```sh
    cargo run -- web-server --cert "$PWD/server.pem" --key "pkcs11:token=$TOKEN;object=$LABEL_2?pin-value=$USER_PIN"
    ```

    The web server runs on port 8443 by default. Use `--port` to use a different value.

1. Verify the cert served by the web server.

    ```sh
    < /dev/null openssl s_client -connect 127.0.0.1:8443
    ```

    This should show the cert chain and have no errors (apart from a verification error because the CA cert is untrusted).

    ```sh
    curl -kD - https://127.0.0.1:8443
    ```

    This should successfully show `curl` completing a TLS handshake and receiving `Hello, world!` from the web server.

1. Use a webclient using the client cert for TLS client auth to connect to the webserver.

    ```sh
    cargo run -- web-client --cert "$PWD/client.pem" --key "pkcs11:token=$TOKEN;object=$LABEL_3?pin-value=$USER_PIN"
    ```

    This should successfully show the client completing a TLS handshake and receiving `Hello, world!` from the web server. The client will print the cert chain it received from the server. The server will also print the client cert chain it received from the client.


# How to run with a TPM

TPM 2.0 hardware currently does not have a fully-functional PKCS#11 implementation. There is [`tpm2-pkcs11`](https://github.com/tpm2-software/tpm2-pkcs11) but it is not yet feature-complete, and does not work on all hardware.

Here are some notes of how to use this demo with a TPM:

- Your hardware may not work with the latest version of `tpm2-pkcs11`, so you may need a specific older version. You may also need specific older versions of [`tpm2-abrmd`,](https://github.com/tpm2-software/tpm2-abrmd) [`tpm2-tss`](https://github.com/tpm2-software/tpm2-tss) and [`tpm2-tools`.](https://github.com/tpm2-software/tpm2-tools) Consult your hardware manufacturer.

- Make sure to initialize the `tpm2-pkcs11` store first:

    ```sh
    tpm2_ptool init --pobj-pin=<>
    ```

    If using a custom store path (`--path <>`), make sure the path is writable by your user.

- `tpm2-pkcs11` only supports RSA 2048-bit keys and ECDSA P-256 keys.


# License

MIT
