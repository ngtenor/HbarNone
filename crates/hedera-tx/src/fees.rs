//! Network-priced Testnet fees. Quotes are obtained before signing, never by
//! submitting a transaction, and no fixed HBAR ceiling is substituted on failure.

use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use prost::Message;
use serde::Deserialize;
use wallet_core::key_import::KeyType;

use crate::{
    SignatureMap, SignaturePair, SignedTransaction, Transaction, TransactionBody,
    TransactionResponse,
};

const MIRROR_URL: &str = "https://testnet.mirrornode.hedera.com/api/v1/network";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeeQuote {
    pub estimated_tinybars: u64,
    pub max_fee_tinybars: u64,
}

/// Approval is required only to exceed the initial estimate-plus-25% maximum.
pub trait FeeApproval: Sync {
    fn approve(
        &self,
        current_maximum: u64,
        required_fee: u64,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

pub fn format_hbar(tinybars: u64) -> String {
    format!("{}.{:08}", tinybars / 100_000_000, tinybars % 100_000_000)
}

#[derive(Deserialize)]
struct Estimate {
    total: u64,
}

#[derive(Deserialize)]
struct ExchangeRate {
    cent_equivalent: u64,
    hbar_equivalent: u64,
    expiration_time: u64,
}

#[derive(Deserialize)]
struct ExchangeRates {
    current_rate: ExchangeRate,
    next_rate: ExchangeRate,
}

impl ExchangeRates {
    fn at(&self, now: u64) -> Result<&ExchangeRate, String> {
        if now < self.current_rate.expiration_time {
            Ok(&self.current_rate)
        } else if now < self.next_rate.expiration_time {
            Ok(&self.next_rate)
        } else {
            Err("The network exchange rates have expired. Try estimating the fee again.".into())
        }
    }
}

fn quote_from_rate(total_tinycents: u64, rate: &ExchangeRate) -> Result<FeeQuote, String> {
    if total_tinycents == 0 || rate.cent_equivalent == 0 || rate.hbar_equivalent == 0 {
        return Err("The network returned an invalid fee estimate or exchange rate.".into());
    }
    // Both tinycents per cent and tinybars per HBAR are 10^8, so they cancel.
    let numerator = u128::from(total_tinycents) * u128::from(rate.hbar_equivalent);
    let estimate = u64::try_from(numerator.div_ceil(u128::from(rate.cent_equivalent)))
        .map_err(|_| "The estimated fee is too large.".to_string())?;
    let maximum = estimate
        .checked_add(estimate.div_ceil(4))
        .ok_or_else(|| "The estimated fee plus 25% is too large.".to_string())?;
    Ok(FeeQuote {
        estimated_tinybars: estimate,
        max_fee_tinybars: maximum,
    })
}

fn estimation_transaction(
    body_bytes: &[u8],
    signing_keys: &[(KeyType, Vec<u8>)],
) -> Result<Transaction, String> {
    if signing_keys.is_empty() {
        return Err("Cannot estimate a transaction without its signing keys.".into());
    }
    let mut body = TransactionBody::decode(body_bytes).map_err(|e| e.to_string())?;
    // Reserve the largest fee encoding so the final fee cannot increase the
    // processing-byte estimate. These signatures are placeholders, not authorisations.
    body.fee = u64::MAX;
    let mut pairs = Vec::new();
    for (kind, public_key) in signing_keys {
        if pairs
            .iter()
            .any(|pair: &SignaturePair| pair.pub_key_prefix == *public_key)
        {
            continue;
        }
        crate::key_message(*kind, public_key)?;
        pairs.push(SignaturePair {
            pub_key_prefix: public_key.clone(),
            ed25519: (*kind == KeyType::Ed25519).then(|| vec![0; 64]),
            ecdsa_secp256k1: (*kind == KeyType::Ecdsa).then(|| vec![0; 64]),
        });
    }
    Ok(Transaction {
        signed_transaction_bytes: SignedTransaction {
            body_bytes: body.encode_to_vec(),
            sig_map: Some(SignatureMap { pairs }),
        }
        .encode_to_vec(),
    })
}

/// Estimates the complete transaction, including all possible collected signatures.
/// The returned body contains the approved-policy maximum: estimate plus 25%.
pub async fn price_body(
    body_bytes: &[u8],
    signing_keys: &[(KeyType, Vec<u8>)],
) -> Result<(Vec<u8>, FeeQuote), String> {
    let transaction = estimation_transaction(body_bytes, signing_keys)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Could not initialise fee estimation: {e}"))?;
    let estimate: Estimate = client
        .post(format!("{MIRROR_URL}/fees?mode=state"))
        .header("Content-Type", "application/protobuf")
        .body(transaction.encode_to_vec())
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Could not estimate the Testnet transaction fee: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Invalid Testnet fee estimate: {e}"))?;
    let rates: ExchangeRates = client
        .get(format!("{MIRROR_URL}/exchangerate"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Could not get the Testnet fee exchange rate: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Invalid Testnet fee exchange rate: {e}"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is before 1970.".to_string())?
        .as_secs();
    let quote = quote_from_rate(estimate.total, rates.at(now)?)?;
    let mut body = TransactionBody::decode(body_bytes).map_err(|e| e.to_string())?;
    body.fee = quote.max_fee_tinybars;
    Ok((body.encode_to_vec(), quote))
}

pub(crate) async fn submit_with_estimated_fee(
    body: &[u8],
    signers: &[(KeyType, &[u8; 32])],
    path: &'static str,
    approval: &impl FeeApproval,
) -> Result<TransactionResponse, String> {
    let keys = signers
        .iter()
        .map(|(kind, secret)| {
            wallet_core::key_import::public_key_bytes(*kind, secret).map(|key| (*kind, key))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (body, quote) = price_body(body, &keys).await?;
    submit_priced_body(
        &body,
        signers,
        &quote,
        path,
        approval,
        crate::submit::submit,
    )
    .await
}

async fn submit_priced_body<F, Fut>(
    body: &[u8],
    signers: &[(KeyType, &[u8; 32])],
    quote: &FeeQuote,
    path: &'static str,
    approval: &impl FeeApproval,
    mut submit: F,
) -> Result<TransactionResponse, String>
where
    F: FnMut(Transaction, &'static str) -> Fut,
    Fut: Future<Output = Result<TransactionResponse, String>>,
{
    let mut decoded = TransactionBody::decode(body).map_err(|e| e.to_string())?;
    if decoded.fee != quote.max_fee_tinybars || quote.max_fee_tinybars == 0 {
        return Err("The transaction does not match the quoted fee.".into());
    }
    let response = submit(sign_valid_body(body, signers)?, path).await?;
    if response.precheck_code != 9 || response.cost <= quote.max_fee_tinybars {
        return Ok(response);
    }
    // A definite precheck rejection was not accepted for consensus. Never
    // retry an ambiguous submission or loop if the increased fee is rejected.
    approval
        .approve(quote.max_fee_tinybars, response.cost)
        .await?;
    decoded.fee = response.cost;
    submit(sign_valid_body(&decoded.encode_to_vec(), signers)?, path).await
}

fn sign_valid_body(body: &[u8], signers: &[(KeyType, &[u8; 32])]) -> Result<Transaction, String> {
    let decoded = TransactionBody::decode(body).map_err(|e| e.to_string())?;
    let start = decoded
        .transaction_id
        .as_ref()
        .and_then(|id| id.valid_start.as_ref())
        .ok_or_else(|| "The priced transaction has no valid start time.".to_string())?;
    let duration = decoded
        .valid_duration
        .as_ref()
        .ok_or_else(|| "The priced transaction has no validity window.".to_string())?;
    let expires = start
        .seconds
        .checked_add(duration.seconds)
        .ok_or_else(|| "The priced transaction has an invalid expiry.".to_string())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is before 1970.".to_string())?
        .as_secs();
    if i128::from(now) >= i128::from(expires) {
        return Err(
            "The transaction expired while estimating or approving the fee. Start again.".into(),
        );
    }
    let encoded = crate::sign_body_multi(body, signers)?;
    Transaction::decode(encoded.as_slice()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tinycents_to_tinybars_then_rounds_up_the_25_percent_buffer() {
        let rate = ExchangeRate {
            cent_equivalent: 190_653,
            hbar_equivalent: 30_000,
            expiration_time: 100,
        };
        let quote = quote_from_rate(500_000_000, &rate).unwrap();
        assert_eq!(quote.estimated_tinybars, 78_676_969);
        assert_eq!(quote.max_fee_tinybars, 98_346_212);
        let rate = ExchangeRate {
            cent_equivalent: 1,
            hbar_equivalent: 1,
            ..rate
        };
        for (total, maximum) in [(1, 2), (4, 5), (5, 7), (2_000_000_000, 2_500_000_000)] {
            assert_eq!(
                quote_from_rate(total, &rate).unwrap().max_fee_tinybars,
                maximum
            );
        }
        assert!(quote_from_rate(u64::MAX, &rate).is_err());
        assert!(quote_from_rate(0, &rate).is_err());
        assert!(
            quote_from_rate(
                1,
                &ExchangeRate {
                    cent_equivalent: 0,
                    ..rate
                }
            )
            .is_err()
        );
    }

    #[test]
    fn uses_next_exchange_rate_only_after_current_rate_expires() {
        let rates: ExchangeRates = serde_json::from_str(
            r#"{"current_rate":{"cent_equivalent":10,"hbar_equivalent":1,"expiration_time":100},
                "next_rate":{"cent_equivalent":20,"hbar_equivalent":1,"expiration_time":200}}"#,
        )
        .unwrap();
        assert_eq!(rates.at(99).unwrap().cent_equivalent, 10);
        assert_eq!(rates.at(100).unwrap().cent_equivalent, 20);
        assert!(rates.at(200).is_err());
        assert!(serde_json::from_str::<Estimate>(r#"{"total":-1}"#).is_err());
        assert!(serde_json::from_str::<Estimate>(r#"{}"#).is_err());
    }

    #[test]
    fn estimation_includes_distinct_signature_types_without_private_signatures() {
        let body = TransactionBody {
            fee: 0,
            ..Default::default()
        }
        .encode_to_vec();
        let keys = vec![
            (KeyType::Ed25519, vec![1; 32]),
            (KeyType::Ecdsa, vec![2; 33]),
            (KeyType::Ed25519, vec![1; 32]),
        ];
        let transaction = estimation_transaction(&body, &keys).unwrap();
        let signed =
            SignedTransaction::decode(transaction.signed_transaction_bytes.as_slice()).unwrap();
        assert_eq!(
            TransactionBody::decode(signed.body_bytes.as_slice())
                .unwrap()
                .fee,
            u64::MAX
        );
        let pairs = signed.sig_map.unwrap().pairs;
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].ed25519, Some(vec![0; 64]));
        assert_eq!(pairs[1].ecdsa_secp256k1, Some(vec![0; 64]));
        assert!(estimation_transaction(&body, &[]).is_err());
    }

    struct Approval {
        accepted: bool,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl FeeApproval for Approval {
        async fn approve(&self, current_maximum: u64, required_fee: u64) -> Result<(), String> {
            assert_eq!(current_maximum, 5_000_000_000);
            assert_eq!(required_fee, 6_000_000_000);
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.accepted {
                Ok(())
            } else {
                Err("Declined".into())
            }
        }
    }

    fn quoted_body() -> (Vec<u8>, FeeQuote) {
        let quote = FeeQuote {
            estimated_tinybars: 4_000_000_000,
            max_fee_tinybars: 5_000_000_000,
        };
        let body = TransactionBody {
            fee: quote.max_fee_tinybars,
            transaction_id: Some(crate::TransactionId {
                valid_start: Some(crate::Timestamp {
                    seconds: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64,
                    nanos: 0,
                }),
                account: None,
            }),
            valid_duration: Some(crate::Duration { seconds: 120 }),
            ..Default::default()
        }
        .encode_to_vec();
        (body, quote)
    }

    #[tokio::test]
    async fn routine_submission_and_non_increases_never_request_fee_approval() {
        for response in [
            Ok(TransactionResponse {
                precheck_code: 0,
                cost: 0,
            }),
            Ok(TransactionResponse {
                precheck_code: 12,
                cost: 0,
            }),
            Ok(TransactionResponse {
                precheck_code: 9,
                cost: 0,
            }),
            Ok(TransactionResponse {
                precheck_code: 9,
                cost: 5_000_000_000,
            }),
            Err("timeout".to_string()),
        ] {
            let (body, quote) = quoted_body();
            let approval = Approval {
                accepted: false,
                calls: Default::default(),
            };
            let mut submissions = 0;
            let result = submit_priced_body(
                &body,
                &[(KeyType::Ed25519, &[7; 32])],
                &quote,
                crate::submit::CREATE_PATH,
                &approval,
                |transaction, path| {
                    submissions += 1;
                    assert_eq!(path, crate::submit::CREATE_PATH);
                    let signed =
                        SignedTransaction::decode(transaction.signed_transaction_bytes.as_slice())
                            .unwrap();
                    assert_eq!(signed.body_bytes, body);
                    std::future::ready(response.clone())
                },
            )
            .await;
            assert_eq!(result, response);
            assert_eq!(submissions, 1);
            assert_eq!(approval.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn only_an_approved_above_buffer_precheck_rejection_is_resigned_and_retried_once() {
        for (accepted, second_code) in [(true, 0), (true, 9), (false, 0)] {
            let (body, quote) = quoted_body();
            let approval = Approval {
                accepted,
                calls: Default::default(),
            };
            let mut signed = Vec::new();
            let mut responses = [
                TransactionResponse {
                    precheck_code: 9,
                    cost: 6_000_000_000,
                },
                TransactionResponse {
                    precheck_code: second_code,
                    cost: 7_000_000_000,
                },
            ]
            .into_iter();
            let result = submit_priced_body(
                &body,
                &[(KeyType::Ed25519, &[7; 32])],
                &quote,
                crate::submit::CREATE_PATH,
                &approval,
                |transaction, _| {
                    signed.push(
                        SignedTransaction::decode(transaction.signed_transaction_bytes.as_slice())
                            .unwrap(),
                    );
                    std::future::ready(Ok(responses.next().expect("unexpected resubmission")))
                },
            )
            .await;
            assert_eq!(approval.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
            if accepted {
                assert_eq!(result.unwrap().precheck_code, second_code);
                assert_eq!(signed.len(), 2);
            } else {
                assert_eq!(result.unwrap_err(), "Declined");
                assert_eq!(signed.len(), 1);
            }
            for (index, transaction) in signed.iter().enumerate() {
                let mut decoded =
                    TransactionBody::decode(transaction.body_bytes.as_slice()).unwrap();
                assert_eq!(
                    decoded.fee,
                    if index == 0 {
                        5_000_000_000
                    } else {
                        6_000_000_000
                    }
                );
                decoded.fee = quote.max_fee_tinybars;
                assert_eq!(decoded.encode_to_vec(), body);
                let signature = ed25519_dalek::Signature::from_slice(
                    transaction.sig_map.as_ref().unwrap().pairs[0]
                        .ed25519
                        .as_ref()
                        .unwrap(),
                )
                .unwrap();
                ed25519_dalek::SigningKey::from_bytes(&[7; 32])
                    .verifying_key()
                    .verify_strict(&transaction.body_bytes, &signature)
                    .unwrap();
            }
        }
    }

    #[test]
    fn expired_fee_requests_are_not_signed() {
        let (body, _) = quoted_body();
        let mut expired = TransactionBody::decode(body.as_slice()).unwrap();
        expired
            .transaction_id
            .as_mut()
            .unwrap()
            .valid_start
            .as_mut()
            .unwrap()
            .seconds = 0;
        assert!(
            sign_valid_body(&expired.encode_to_vec(), &[(KeyType::Ed25519, &[7; 32])])
                .unwrap_err()
                .contains("expired")
        );
    }

    #[tokio::test]
    #[ignore = "queries public Testnet fee estimation only; never submits a transaction"]
    async fn live_testnet_estimates_an_unsigned_creation() {
        let body = crate::create_account_body_bytes(&crate::CreateAccountParams {
            payer: (0, 0, 1001),
            key: crate::key_message(KeyType::Ed25519, &[1; 32]).unwrap(),
            initial_tinybars: 100_000_000,
            node: crate::submit::TESTNET_NODE_ACCOUNT,
            valid_start_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                - 10,
            valid_start_nanos: 0,
            max_fee_tinybars: 0,
            memo: String::new(),
        })
        .unwrap();
        let keys = [(KeyType::Ed25519, vec![2; 32])];
        let (priced, quote) = price_body(&body, &keys).await.unwrap();
        let mut priced = TransactionBody::decode(priced.as_slice()).unwrap();
        assert!(quote.estimated_tinybars > 0);
        assert_eq!(
            quote.max_fee_tinybars,
            quote.estimated_tinybars + quote.estimated_tinybars.div_ceil(4),
        );
        assert_eq!(priced.fee, quote.max_fee_tinybars);
        priced.fee = 0;
        assert_eq!(priced.encode_to_vec(), body);
        println!(
            "Estimate: {} HBAR; maximum with 25%: {} HBAR",
            format_hbar(quote.estimated_tinybars),
            format_hbar(quote.max_fee_tinybars)
        );
    }
}
