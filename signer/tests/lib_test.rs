use std::convert::TryFrom;

use bip39::{Language, Seed};
use bls_signatures::Serialize;
use forest_address::Address;
use forest_encoding::{to_vec, Cbor};
use forest_message::UnsignedMessage;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

use filecoin_signer::api::{MessageParams, MessageTxAPI, UnsignedMessageAPI};
use filecoin_signer::signature::{Signature, SignatureBLS};
use filecoin_signer::*;

use extras::multisig;

mod common;

const SIGNED_MESSAGE_CBOR: &str =
    "828a005501fd1d0f4dfcd7e99afcb99a8326b7dc459d32c62855011eaf1c8a4bbfeeb0870b1745b1f57503470b71160144000186a01909c4420001420001004058420106398485060ca2a4deb97027f518f45569360c3873a4303926fa6909a7299d4c55883463120836358ff3396882ee0dc2cf15961bd495cdfb3de1ee2e8bd3768e01";

#[test]
fn decode_key() {
    let test_value = common::load_test_vectors("../test_vectors/wallet.json").unwrap();
    let private_key = test_value["private_key"].as_str().unwrap();

    let pk = PrivateKey::try_from(private_key.to_string()).unwrap();
    assert_eq!(base64::encode(&pk.0), private_key.to_string());
}

#[test]
fn generate_mnemonic() {
    let mnemonic = key_generate_mnemonic().expect("could not generate mnemonic");
    println!("{}", mnemonic.0);

    let word_count = mnemonic.0.split_ascii_whitespace().count();
    assert_eq!(word_count, 24)
}

#[test]
fn derive_key() {
    let test_value = common::load_test_vectors("../test_vectors/wallet.json").unwrap();
    let mnemonic = test_value["mnemonic"].as_str().unwrap();
    let private_key = test_value["private_key"].as_str().unwrap();
    let language_code = test_value["language_code"].as_str().unwrap();

    let extended_key = key_derive(&mnemonic, "m/44'/461'/0/0/0", "", language_code).unwrap();

    assert_eq!(
        base64::encode(&extended_key.private_key.0),
        private_key.to_string()
    );
}

#[test]
fn derive_key_password() {
    let test_value = common::load_test_vectors("../test_vectors/wallet.json").unwrap();
    let mnemonic = test_value["mnemonic"].as_str().unwrap();
    let password = "password".to_string();
    let path = "m/44'/461'/0/0/0".to_string();
    let language_code = test_value["language_code"].as_str().unwrap();

    let m = bip39::Mnemonic::from_phrase(&mnemonic, Language::English).unwrap();

    let seed = Seed::new(&m, &password);

    let extended_key_expected = key_derive_from_seed(seed.as_bytes(), &path).unwrap();

    let extended_key = key_derive(&mnemonic, &path, &password, &language_code).unwrap();

    assert_eq!(
        base64::encode(&extended_key.private_key.0),
        base64::encode(&extended_key_expected.private_key.0)
    );
}

#[test]
fn derive_key_from_seed() {
    let test_value = common::load_test_vectors("../test_vectors/wallet.json").unwrap();
    let mnemonic = Mnemonic(test_value["mnemonic"].as_str().unwrap().to_string());
    let private_key = test_value["private_key"].as_str().unwrap();

    let mnemonic = bip39::Mnemonic::from_phrase(&mnemonic.0, Language::English).unwrap();

    let seed = Seed::new(&mnemonic, "");

    let extended_key = key_derive_from_seed(seed.as_bytes(), "m/44'/461'/0/0/0").unwrap();

    assert_eq!(
        base64::encode(&extended_key.private_key.0),
        private_key.to_string()
    );
}

#[test]
fn test_key_recover_testnet() {
    let test_value = common::load_test_vectors("../test_vectors/wallet.json").unwrap();
    let private_key = test_value["private_key"].as_str().unwrap();

    let pk = PrivateKey::try_from(private_key.to_string()).unwrap();
    let testnet = true;

    let recovered_key = key_recover(&pk, testnet).unwrap();

    assert_eq!(
        base64::encode(&recovered_key.private_key.0),
        private_key.to_string()
    );

    assert_eq!(
        &recovered_key.address,
        "t1d2xrzcslx7xlbbylc5c3d5lvandqw4iwl6epxba"
    );
}

#[test]
fn test_key_recover_mainnet() {
    let test_value = common::load_test_vectors("../test_vectors/wallet.json").unwrap();
    let private_key = test_value["private_key"].as_str().unwrap();
    let address = test_value["childs"][3]["address"].as_str().unwrap();

    let pk = PrivateKey::try_from(private_key.to_string()).unwrap();
    let testnet = false;

    let recovered_key = key_recover(&pk, testnet).unwrap();

    assert_eq!(
        base64::encode(&recovered_key.private_key.0),
        private_key.to_string()
    );

    assert_eq!(&recovered_key.address, &address);
}

#[test]
fn parse_unsigned_transaction() {
    let test_value = common::load_test_vectors("../test_vectors/txs.json").unwrap();
    let cbor = test_value[0]["cbor"].as_str().unwrap();
    let to_expected = test_value[0]["transaction"]["to"].as_str().unwrap();

    let cbor_data = CborBuffer(hex::decode(&cbor).unwrap());

    let unsigned_tx = transaction_parse(&cbor_data, true).expect("FIX ME");
    let to = match unsigned_tx {
        MessageTxAPI::UnsignedMessageAPI(tx) => tx.to,
        MessageTxAPI::SignedMessageAPI(_) => panic!("Should be a Unsigned Message!"),
    };

    assert_eq!(to, to_expected.to_string());
}

#[test]
fn parse_signed_transaction() {
    // TODO: new test vector
    let cbor_data = CborBuffer(hex::decode(SIGNED_MESSAGE_CBOR).unwrap());

    let signed_tx = transaction_parse(&cbor_data, true).expect("Could not parse");
    let signature = match signed_tx {
        MessageTxAPI::UnsignedMessageAPI(_) => panic!("Should be a Signed Message!"),
        MessageTxAPI::SignedMessageAPI(tx) => tx.signature,
    };

    assert_eq!(
        hex::encode(&signature.data),
        "06398485060ca2a4deb97027f518f45569360c3873a4303926fa6909a7299d4c55883463120836358ff3396882ee0dc2cf15961bd495cdfb3de1ee2e8bd3768e01".to_string()
    );
}

#[test]
fn parse_transaction_with_network() {
    let test_value = common::load_test_vectors("../test_vectors/txs.json").unwrap();
    let tc = test_value[1].to_owned();
    let cbor = tc["cbor"].as_str().unwrap();
    let testnet = tc["testnet"].as_bool().unwrap();
    let to_expected = tc["transaction"]["to"].as_str().unwrap();
    let from_expected = tc["transaction"]["from"].as_str().unwrap();

    let cbor_data = CborBuffer(hex::decode(&cbor).unwrap());

    let unsigned_tx_mainnet = transaction_parse(&cbor_data, testnet).expect("Could not parse");
    let (to, from) = match unsigned_tx_mainnet {
        MessageTxAPI::UnsignedMessageAPI(tx) => (tx.to, tx.from),
        MessageTxAPI::SignedMessageAPI(_) => panic!("Should be a Unsigned Message!"),
    };

    assert_eq!(to, to_expected.to_string());
    assert_eq!(from, from_expected.to_string());
}

#[test]
fn parse_transaction_with_network_testnet() {
    let test_value = common::load_test_vectors("../test_vectors/txs.json").unwrap();
    let tc = test_value[0].to_owned();
    let cbor = tc["cbor"].as_str().unwrap();
    let testnet = tc["testnet"].as_bool().unwrap();
    let to_expected = tc["transaction"]["to"].as_str().unwrap();
    let from_expected = tc["transaction"]["from"].as_str().unwrap();

    let cbor_data = CborBuffer(hex::decode(&cbor).unwrap());

    let unsigned_tx_testnet = transaction_parse(&cbor_data, testnet).expect("Could not parse");
    let (to, from) = match unsigned_tx_testnet {
        MessageTxAPI::UnsignedMessageAPI(tx) => (tx.to, tx.from),
        MessageTxAPI::SignedMessageAPI(_) => panic!("Should be a Unsigned Message!"),
    };

    assert_eq!(to, to_expected.to_string());
    assert_eq!(from, from_expected.to_string());
}

#[test]
fn parse_transaction_signed_with_network() {
    // TODO: test vector for signed message
    let cbor_data = CborBuffer(hex::decode(SIGNED_MESSAGE_CBOR).unwrap());

    let signed_tx_mainnet = transaction_parse(&cbor_data, false).expect("Could not parse");
    let (to, from) = match signed_tx_mainnet {
        MessageTxAPI::UnsignedMessageAPI(_) => panic!("Should be a Signed Message!"),
        MessageTxAPI::SignedMessageAPI(tx) => (tx.message.to, tx.message.from),
    };

    println!("{}", to);
    assert_eq!(to, "f17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy".to_string());
    assert_eq!(
        from,
        "f1d2xrzcslx7xlbbylc5c3d5lvandqw4iwl6epxba".to_string()
    );
}

#[test]
fn parse_transaction_signed_with_network_testnet() {
    // TODO: test vector for signed message
    let cbor_data = CborBuffer(hex::decode(SIGNED_MESSAGE_CBOR).unwrap());

    let signed_tx_testnet = transaction_parse(&cbor_data, true).expect("Could not parse");
    let (to, from) = match signed_tx_testnet {
        MessageTxAPI::UnsignedMessageAPI(_) => panic!("Should be a Signed Message!"),
        MessageTxAPI::SignedMessageAPI(tx) => (tx.message.to, tx.message.from),
    };

    assert_eq!(to, "t17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy".to_string());
    assert_eq!(
        from,
        "t1d2xrzcslx7xlbbylc5c3d5lvandqw4iwl6epxba".to_string()
    );
}

#[test]
fn verify_invalid_signature() {
    let test_value = common::load_test_vectors("../test_vectors/verify_signature.json").unwrap();
    let private_key = test_value["verify_invalid_signature"]["private_key"]
        .as_str()
        .unwrap();
    let message = test_value["verify_invalid_signature"]["message"].to_owned();

    // Path 44'/461'/0/0/0
    let pk = PrivateKey::try_from(private_key.to_string()).unwrap();
    let message_user_api: UnsignedMessageAPI =
        serde_json::from_value(message).expect("Could not serialize unsigned message");

    // Sign
    let signature = transaction_sign_raw(&message_user_api, &pk).unwrap();

    // Verify
    let message = forest_message::UnsignedMessage::try_from(&message_user_api)
        .expect("Could not serialize unsigned message");
    let message_cbor = CborBuffer(to_vec(&message).unwrap());

    let valid_signature = verify_signature(&signature, &message_cbor);
    assert!(valid_signature.unwrap());

    // Tampered signature and look if it valid
    let mut sig = signature.as_bytes();
    sig[5] = 0x01;
    sig[34] = 0x00;

    let tampered_signature = Signature::try_from(sig).expect("FIX ME");

    let valid_signature = verify_signature(&tampered_signature, &message_cbor);
    assert!(valid_signature.is_err() || !valid_signature.unwrap());
}

#[test]
fn sign_bls_transaction() {
    let test_value = common::load_test_vectors("../test_vectors/bls_wallet.json").unwrap();

    // Get address
    let bls_pubkey = hex::decode(test_value["bls_public_key"].as_str().unwrap()).unwrap();
    let bls_address = Address::new_bls(bls_pubkey.as_slice()).unwrap();

    // Get BLS private key
    let bls_key =
        PrivateKey::try_from(test_value["bls_private_key"].as_str().unwrap().to_string()).unwrap();

    dbg!(bls_address.to_string());

    // Prepare message with BLS address
    let message = UnsignedMessageAPI {
        to: "t17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy".to_string(),
        from: bls_address.to_string(),
        nonce: 1,
        value: "100000".to_string(),
        gas_limit: 25000,
        gas_fee_cap: "2500".to_string(),
        gas_premium: "2500".to_string(),
        method: 0,
        params: "".to_string(),
    };

    let raw_sig = transaction_sign_raw(&message, &bls_key).unwrap();

    dbg!(hex::encode(raw_sig.as_bytes()));

    let sig = bls_signatures::Signature::from_bytes(&raw_sig.as_bytes()).expect("FIX ME");

    let bls_pk = bls_signatures::PublicKey::from_bytes(&bls_pubkey).unwrap();

    let message = UnsignedMessage::try_from(&message).expect("FIX ME");
    let message_cbor = message.marshal_cbor().expect("FIX ME");

    dbg!(hex::encode(&message_cbor));

    assert!(bls_pk.verify(sig, &message.to_signing_bytes()));
}

#[test]
fn test_verify_bls_signature() {
    let test_value = common::load_test_vectors("../test_vectors/bls_signature.json").unwrap();

    let sig = Signature::try_from(test_value["sig"].as_str().unwrap().to_string()).unwrap();
    let message =
        CborBuffer(hex::decode(test_value["cbor"].as_str().unwrap().to_string()).unwrap());

    let result = verify_signature(&sig, &message).unwrap();

    assert!(result);
}

#[test]
fn test_verify_aggregated_signature() {
    // sign 3 messages
    let num_messages = 3;

    let mut rng = ChaCha8Rng::seed_from_u64(12);

    // generate private keys
    let private_keys: Vec<_> = (0..num_messages)
        .map(|_| bls_signatures::PrivateKey::generate(&mut rng))
        .collect();

    // generate messages
    let messages: Vec<UnsignedMessageAPI> = (0..num_messages)
        .map(|i| {
            //Prepare transaction
            let bls_public_key = private_keys[i].public_key();
            let bls_address = Address::new_bls(&bls_public_key.as_bytes()).unwrap();

            UnsignedMessageAPI {
                to: "t17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy".to_string(),
                from: bls_address.to_string(),
                nonce: 1,
                value: "100000".to_string(),
                gas_limit: 25000,
                gas_fee_cap: "2500".to_string(),
                gas_premium: "2500".to_string(),
                method: 0,
                params: "".to_string(),
            }
        })
        .collect();

    // sign messages
    let sigs: Vec<bls_signatures::Signature>;
    sigs = messages
        .par_iter()
        .zip(private_keys.par_iter())
        .map(|(message, pk)| {
            let private_key = PrivateKey::try_from(pk.as_bytes()).expect("FIX ME");
            let raw_sig = transaction_sign_raw(message, &private_key).unwrap();

            bls_signatures::Serialize::from_bytes(&raw_sig.as_bytes()).expect("FIX ME")
        })
        .collect::<Vec<bls_signatures::Signature>>();

    // serialize messages
    let cbor_messages: Vec<CborBuffer>;
    cbor_messages = messages
        .par_iter()
        .map(|message| transaction_serialize(message).unwrap())
        .collect::<Vec<CborBuffer>>();

    let aggregated_signature = bls_signatures::aggregate(&sigs).expect("FIX ME");

    let sig = SignatureBLS::try_from(aggregated_signature.as_bytes()).expect("FIX ME");

    assert!(verify_aggregated_signature(&sig, &cbor_messages[..]).unwrap());
}

#[test]
fn payment_channel_creation_bls_signing() {
    let test_value = common::load_test_vectors("../test_vectors/payment_channel.json").unwrap();
    let tc_creation_bls = test_value["creation"]["bls"].to_owned();

    let from_key = tc_creation_bls["private_key"].as_str().unwrap();
    let bls_key = PrivateKey::try_from(from_key.to_string()).unwrap();
    let from_pkey = tc_creation_bls["public_key"].as_str().unwrap();

    let pch_create_message_api = create_pymtchan(
        tc_creation_bls["constructor_params"]["from"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_creation_bls["constructor_params"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_creation_bls["message"]["value"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_creation_bls["message"]["nonce"].as_u64().unwrap(),
        tc_creation_bls["message"]["gaslimit"].as_i64().unwrap(),
        tc_creation_bls["message"]["gasfeecap"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_creation_bls["message"]["gaspremium"]
            .as_str()
            .unwrap()
            .to_string(),
    )
    .unwrap();

    let pch_create_message_expected: UnsignedMessageAPI =
        serde_json::from_value(tc_creation_bls["message"].to_owned()).unwrap();

    assert_eq!(
        serde_json::to_string(&pch_create_message_expected.params).unwrap(),
        serde_json::to_string(&pch_create_message_api.params).unwrap()
    );

    // First check transaction_serialize() in creating an unsigned message
    let _ = transaction_serialize(&pch_create_message_api).unwrap();

    let unsigned_message = UnsignedMessage::try_from(&pch_create_message_api).unwrap();
    let bls_signing_bytes = unsigned_message.to_signing_bytes();

    // Now check that we can generate a correct signature
    let sig = transaction_sign_raw(&pch_create_message_api, &bls_key).unwrap();

    let bls_pkey = bls_signatures::PublicKey::from_bytes(&hex::decode(from_pkey).unwrap()).unwrap();

    let bls_sig = bls_signatures::Serialize::from_bytes(&sig.as_bytes()).expect("FIX ME");

    assert!(bls_pkey.verify(bls_sig, &bls_signing_bytes));
}

#[test]
fn payment_channel_creation_secp256k1_signing() {
    let test_value = common::load_test_vectors("../test_vectors/payment_channel.json").unwrap();
    let tc_creation_secp256k1 = test_value["creation"]["secp256k1"].to_owned();

    let from_key = tc_creation_secp256k1["private_key"]
        .as_str()
        .unwrap()
        .to_string();
    let _from_pkey = tc_creation_secp256k1["public_key"]
        .as_str()
        .unwrap()
        .to_string();
    let privkey = PrivateKey::try_from(from_key).unwrap();

    let pch_create_message_api: UnsignedMessageAPI =
        serde_json::from_value(tc_creation_secp256k1["message"].to_owned())
            .expect("Could not serialize unsigned message");
    // TODO:  ^^^ this is an error, these lines are duplicated.  First one should have called create_pymtchan()

    let signed_message_result = transaction_sign(&pch_create_message_api, &privkey).unwrap();
    // TODO:  how do I check the signature of a transaction_sign() result

    // Check the raw bytes match the test vector cbor
    let _cbor_result_unsigned_msg = transaction_serialize(&signed_message_result.message).unwrap();
}

#[test]
fn payment_channel_update() {
    let test_value = common::load_test_vectors("../test_vectors/payment_channel.json").unwrap();
    let tc_update_secp256k1 = test_value["update"]["secp256k1"].to_owned();

    let from_key = tc_update_secp256k1["private_key"]
        .as_str()
        .unwrap()
        .to_string();
    let privkey = PrivateKey::try_from(from_key).unwrap();

    let pch_update_message_unsigned_api = update_pymtchan(
        tc_update_secp256k1["message"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_update_secp256k1["message"]["from"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_update_secp256k1["voucher_base64"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_update_secp256k1["message"]["nonce"].as_u64().unwrap(),
        tc_update_secp256k1["message"]["gaslimit"].as_i64().unwrap(),
        tc_update_secp256k1["message"]["gasfeecap"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_update_secp256k1["message"]["gaspremium"]
            .as_str()
            .unwrap()
            .to_string(),
    )
    .unwrap();

    let pch_update_message_unsigned_expected: UnsignedMessageAPI =
        serde_json::from_value(tc_update_secp256k1["message"].to_owned())
            .expect("Could not serialize unsigned message");

    assert_eq!(
        serde_json::to_string(&pch_update_message_unsigned_expected).unwrap(),
        serde_json::to_string(&pch_update_message_unsigned_api).unwrap()
    );

    // Sign
    let signature = transaction_sign_raw(&pch_update_message_unsigned_api, &privkey).unwrap();

    // Verify
    let message = forest_message::UnsignedMessage::try_from(&pch_update_message_unsigned_api)
        .expect("Could not serialize unsigned message");
    let message_cbor = CborBuffer(to_vec(&message).unwrap());

    let valid_signature = verify_signature(&signature, &message_cbor);
    assert!(valid_signature.unwrap());
}

#[test]
fn payment_channel_settle() {
    let test_value = common::load_test_vectors("../test_vectors/payment_channel.json").unwrap();
    let tc_settle_secp256k1 = test_value["settle"]["secp256k1"].to_owned();

    let from_key = tc_settle_secp256k1["private_key"]
        .as_str()
        .unwrap()
        .to_string();
    let privkey = PrivateKey::try_from(from_key).unwrap();

    let pch_settle_message_unsigned_api = settle_pymtchan(
        tc_settle_secp256k1["message"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_settle_secp256k1["message"]["from"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_settle_secp256k1["message"]["nonce"].as_u64().unwrap(),
        tc_settle_secp256k1["message"]["gaslimit"].as_i64().unwrap(),
        tc_settle_secp256k1["message"]["gasfeecap"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_settle_secp256k1["message"]["gaspremium"]
            .as_str()
            .unwrap()
            .to_string()
            .to_string(),
    )
    .unwrap();

    let pch_settle_message_unsigned_expected: UnsignedMessageAPI =
        serde_json::from_value(tc_settle_secp256k1["message"].to_owned())
            .expect("Could not serialize unsigned message");

    assert_eq!(
        serde_json::to_string(&pch_settle_message_unsigned_expected).unwrap(),
        serde_json::to_string(&pch_settle_message_unsigned_api).unwrap()
    );

    // Sign
    let signature = transaction_sign_raw(&pch_settle_message_unsigned_api, &privkey).unwrap();

    // Verify
    let message = forest_message::UnsignedMessage::try_from(&pch_settle_message_unsigned_api)
        .expect("Could not serialize unsigned message");
    let message_cbor = CborBuffer(to_vec(&message).unwrap());

    let valid_signature = verify_signature(&signature, &message_cbor);
    assert!(valid_signature.unwrap());
}

#[test]
fn payment_channel_collect() {
    let test_value = common::load_test_vectors("../test_vectors/payment_channel.json").unwrap();
    let tc_collect_secp256k1 = test_value["collect"]["secp256k1"].to_owned();

    let from_key = tc_collect_secp256k1["private_key"]
        .as_str()
        .unwrap()
        .to_string();
    let privkey = PrivateKey::try_from(from_key).unwrap();

    let pch_collect_message_unsigned_api = collect_pymtchan(
        tc_collect_secp256k1["message"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_collect_secp256k1["message"]["from"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_collect_secp256k1["message"]["nonce"].as_u64().unwrap(),
        tc_collect_secp256k1["message"]["gaslimit"]
            .as_i64()
            .unwrap(),
        tc_collect_secp256k1["message"]["gasfeecap"]
            .as_str()
            .unwrap()
            .to_string(),
        tc_collect_secp256k1["message"]["gaspremium"]
            .as_str()
            .unwrap()
            .to_string(),
    )
    .unwrap();

    let pch_collect_message_unsigned_expected: UnsignedMessageAPI =
        serde_json::from_value(tc_collect_secp256k1["message"].to_owned())
            .expect("Could not serialize unsigned message");

    assert_eq!(
        serde_json::to_string(&pch_collect_message_unsigned_expected).unwrap(),
        serde_json::to_string(&pch_collect_message_unsigned_api).unwrap()
    );

    // Sign
    let signature = transaction_sign_raw(&pch_collect_message_unsigned_api, &privkey).unwrap();

    // Verify
    let message = forest_message::UnsignedMessage::try_from(&pch_collect_message_unsigned_api)
        .expect("Could not serialize unsigned message");
    let message_cbor = CborBuffer(to_vec(&message).unwrap());

    let valid_signature = verify_signature(&signature, &message_cbor);
    assert!(valid_signature.unwrap());
}

#[test]
fn test_sign_voucher() {
    let wallet = common::load_test_vectors("../test_vectors/wallet.json").unwrap();
    // TODO: the privatekey should be added to voucher.json to keep test vectors seperated
    let mnemonic = wallet["mnemonic"].as_str().unwrap();
    let language_code = wallet["language_code"].as_str().unwrap();

    let extended_key = key_derive(mnemonic, "m/44'/461'/0/0/0", "", language_code).unwrap();

    let test_value = common::load_test_vectors("../test_vectors/voucher.json").unwrap();
    let voucher_value = test_value["sign"]["voucher"].to_owned();

    let voucher = create_voucher(
        voucher_value["payment_channel_address"]
            .as_str()
            .unwrap()
            .to_string(),
        voucher_value["time_lock_min"].as_i64().unwrap(),
        voucher_value["time_lock_max"].as_i64().unwrap(),
        voucher_value["amount"].as_str().unwrap().to_string(),
        voucher_value["lane"].as_u64().unwrap(),
        voucher_value["nonce"].as_u64().unwrap(),
        voucher_value["min_settle_height"].as_i64().unwrap(),
    )
    .unwrap();

    let signed_voucher = sign_voucher(voucher, &extended_key.private_key).unwrap();

    assert_eq!(
        signed_voucher,
        test_value["sign"]["signed_voucher_base64"]
            .as_str()
            .unwrap()
    );
}

#[test]
fn support_multisig_create() {
    let test_value = common::load_test_vectors("../test_vectors/multisig.json").unwrap();

    let multisig_create_message_api = create_multisig(
        test_value["create"]["message"]["from"]
            .as_str()
            .unwrap()
            .to_string(),
        vec![
            test_value["create"]["constructor_params"]["signers"][0]
                .as_str()
                .unwrap()
                .to_string(),
            test_value["create"]["constructor_params"]["signers"][1]
                .as_str()
                .unwrap()
                .to_string(),
        ],
        test_value["create"]["message"]["value"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["create"]["constructor_params"]["num_approvals_threshold"]
            .as_i64()
            .unwrap(),
        test_value["create"]["message"]["nonce"].as_u64().unwrap(),
        test_value["create"]["constructor_params"]["unlock_duration"]
            .as_i64()
            .unwrap(),
        test_value["create"]["constructor_params"]["start_epoch"]
            .as_i64()
            .unwrap(),
        test_value["create"]["message"]["gaslimit"]
            .as_i64()
            .unwrap(),
        test_value["create"]["message"]["gasfeecap"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["create"]["message"]["gaspremium"]
            .as_str()
            .unwrap()
            .to_string(),
    )
    .unwrap();

    let multisig_create_message_expected: UnsignedMessageAPI =
        serde_json::from_value(test_value["create"]["message"].to_owned()).unwrap();

    assert_eq!(
        serde_json::to_string(&multisig_create_message_expected).unwrap(),
        serde_json::to_string(&multisig_create_message_api).unwrap()
    );

    let result = transaction_serialize(&multisig_create_message_api).unwrap();

    assert_eq!(
        hex::encode(&result),
        test_value["create"]["cbor"].as_str().unwrap()
    );
}

#[test]
fn support_multisig_propose_message() {
    let test_value = common::load_test_vectors("../test_vectors/multisig.json").unwrap();

    let multisig_proposal_message_api = proposal_multisig_message(
        test_value["propose"]["message"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["propose"]["proposal_params"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["propose"]["message"]["from"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["propose"]["proposal_params"]["value"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["propose"]["message"]["nonce"].as_u64().unwrap(),
        test_value["propose"]["message"]["gaslimit"]
            .as_i64()
            .unwrap(),
        test_value["propose"]["message"]["gasfeecap"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["propose"]["message"]["gaspremium"]
            .as_str()
            .unwrap()
            .to_string(),
    )
    .unwrap();

    let multisig_proposal_message_expected: UnsignedMessageAPI =
        serde_json::from_value(test_value["propose"]["message"].to_owned()).unwrap();

    assert_eq!(
        serde_json::to_string(&multisig_proposal_message_expected).unwrap(),
        serde_json::to_string(&multisig_proposal_message_api).unwrap()
    );

    let result = transaction_serialize(&multisig_proposal_message_api).unwrap();

    assert_eq!(hex::encode(&result), test_value["propose"]["cbor"]);
}

#[test]
fn support_multisig_approve_message() {
    let test_value = common::load_test_vectors("../test_vectors/multisig.json").unwrap();

    let multisig_approval_message_api = approve_multisig_message(
        test_value["approve"]["message"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["approve"]["approval_params"]["txn_id"]
            .as_i64()
            .unwrap(),
        test_value["approve"]["proposal_params"]["requester"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["approve"]["proposal_params"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["approve"]["proposal_params"]["value"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["approve"]["message"]["from"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["approve"]["message"]["nonce"].as_u64().unwrap(),
        test_value["approve"]["message"]["gaslimit"]
            .as_i64()
            .unwrap(),
        test_value["approve"]["message"]["gasfeecap"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["approve"]["message"]["gaspremium"]
            .as_str()
            .unwrap()
            .to_string(),
    )
    .unwrap();

    let multisig_approval_message_expected: UnsignedMessageAPI =
        serde_json::from_value(test_value["approve"]["message"].to_owned()).unwrap();

    assert_eq!(
        serde_json::to_string(&multisig_approval_message_expected).unwrap(),
        serde_json::to_string(&multisig_approval_message_api).unwrap()
    );

    let result = transaction_serialize(&multisig_approval_message_api).unwrap();

    assert_eq!(hex::encode(&result), test_value["approve"]["cbor"]);
}

#[test]
fn support_multisig_cancel_message() {
    let test_value = common::load_test_vectors("../test_vectors/multisig.json").unwrap();

    let multisig_cancel_message_api = cancel_multisig_message(
        test_value["cancel"]["message"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["cancel"]["cancel_params"]["txn_id"]
            .as_i64()
            .unwrap(),
        test_value["cancel"]["proposal_params"]["requester"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["cancel"]["proposal_params"]["to"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["cancel"]["proposal_params"]["value"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["cancel"]["message"]["from"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["cancel"]["message"]["nonce"].as_u64().unwrap(),
        test_value["cancel"]["message"]["gaslimit"]
            .as_i64()
            .unwrap(),
        test_value["cancel"]["message"]["gasfeecap"]
            .as_str()
            .unwrap()
            .to_string(),
        test_value["cancel"]["message"]["gaspremium"]
            .as_str()
            .unwrap()
            .to_string(),
    )
    .unwrap();

    let multisig_cancel_message_expected: UnsignedMessageAPI =
        serde_json::from_value(test_value["cancel"]["message"].to_owned()).unwrap();

    assert_eq!(
        serde_json::to_string(&multisig_cancel_message_expected).unwrap(),
        serde_json::to_string(&multisig_cancel_message_api).unwrap()
    );

    let result = transaction_serialize(&multisig_cancel_message_api).unwrap();

    assert_eq!(hex::encode(&result), test_value["cancel"]["cbor"]);
}

#[test]
fn test_verify_voucher_signature() {
    let test_value = common::load_test_vectors("../test_vectors/voucher.json").unwrap();

    let voucher_base64_string = test_value["verify"]["signed_voucher_base64"]
        .as_str()
        .unwrap()
        .to_string();
    let address_signer = test_value["verify"]["address_signer"]
        .as_str()
        .unwrap()
        .to_string();

    let result = verify_voucher_signature(voucher_base64_string, address_signer).expect("FIX ME");

    assert!(result);
}

#[test]
fn test_get_cid() {
    let test_value = common::load_test_vectors("../test_vectors/get_cid.json").unwrap();

    let expected_cid = test_value["cid"].as_str().unwrap().to_string();
    let message_api: MessageTxAPI = serde_json::from_value(test_value["signed_message"].to_owned())
        .expect("couldn't serialize signed message");

    let cid = get_cid(message_api).unwrap();

    assert_eq!(cid, expected_cid);
}

#[test]
fn test_multisig_v1_deserialize() {
    let expected_params = multisig::ConstructorParams {
        signers: vec![Address::from_bytes(
            &hex::decode("01D75AB2B78BB2FEB1CF86B1412E96916D805B40C3").unwrap(),
        )
        .unwrap()],
        num_approvals_threshold: 1,
        unlock_duration: 0,
        start_epoch: 0,
    };
    let params = deserialize_constructor_params(
        "g4FVAddasreLsv6xz4axQS6WkW2AW0DDAQA=".to_string(),
        "fil/1/multisig".to_string(),
    )
    .unwrap();

    assert_eq!(
        params,
        MessageParams::ConstructorParamsMultisig(expected_params.into())
    );
}
