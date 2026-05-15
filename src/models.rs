use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// --- Input row (CSV deserialization) ---

#[derive(Debug, Deserialize)]
pub struct TransactionRow {
    #[serde(rename = "type")]
    pub tx_type: String,
    pub client: u16,
    pub tx: u32,
    #[serde(default, deserialize_with = "deserialize_optional_decimal")]
    pub amount: Option<Decimal>,
}

fn deserialize_optional_decimal<'de, D>(deserializer: D) -> Result<Option<Decimal>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    if s.trim().is_empty() {
        Ok(None)
    } else {
        let value = Decimal::from_str(s.trim()).map_err(serde::de::Error::custom)?;
        if value.scale() > 4 {
            return Err(serde::de::Error::custom(
                "amount must have at most 4 decimal places",
            ));
        }
        Ok(Some(value))
    }
}

// --- Internal transaction record (for dispute lookups) ---

// Copy enables zero-cost field extraction in dispute/resolve/chargeback
// — avoids cloning the entire TransactionRecord (anti-pattern)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransactionKind {
    Deposit,
    Withdrawal,
}

#[derive(Debug)]
pub struct TransactionRecord {
    pub kind: TransactionKind,
    pub client_id: u16,
    pub amount: Decimal,
    pub under_dispute: bool,
}

// --- Client account (CSV serialization) ---

#[derive(Debug, Serialize)]
pub struct Client {
    #[serde(rename = "client")]
    pub id: u16,
    #[serde(serialize_with = "serialize_decimal_4dp")]
    pub available: Decimal,
    #[serde(serialize_with = "serialize_decimal_4dp")]
    pub held: Decimal,
    #[serde(serialize_with = "serialize_decimal_4dp")]
    pub total: Decimal,
    pub locked: bool,
}

fn serialize_decimal_4dp<S>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&format!("{:.4}", value))
}

impl Client {
    pub fn new(id: u16) -> Self {
        Self {
            id,
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            total: Decimal::ZERO,
            locked: false,
        }
    }

    // SRP: fund movement logic belongs on Client, not on PaymentEngine
    pub(crate) fn apply_dispute(&mut self, kind: TransactionKind, amount: Decimal) {
        match kind {
            TransactionKind::Deposit => {
                self.available -= amount;
                self.held += amount;
            }
            TransactionKind::Withdrawal => {
                self.available += amount;
                self.held += amount;
                self.total += amount;
            }
        }
    }

    pub(crate) fn apply_resolve(&mut self, kind: TransactionKind, amount: Decimal) {
        match kind {
            TransactionKind::Deposit => {
                self.available += amount;
                self.held -= amount;
            }
            TransactionKind::Withdrawal => {
                self.available -= amount;
                self.held -= amount;
                self.total -= amount;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_new_has_zero_balances() {
        let c = Client::new(42);
        assert_eq!(c.id, 42);
        assert_eq!(c.available, Decimal::ZERO);
        assert_eq!(c.held, Decimal::ZERO);
        assert_eq!(c.total, Decimal::ZERO);
        assert!(!c.locked);
    }

    #[test]
    fn deserialize_optional_decimal_with_value() {
        let result: Option<Decimal> =
            deserialize_optional_decimal(serde_json::Value::String("1.2345".into())).unwrap();
        assert_eq!(result, Some(Decimal::new(12345, 4)));
    }

    #[test]
    fn deserialize_optional_decimal_empty() {
        let result: Option<Decimal> =
            deserialize_optional_decimal(serde_json::Value::String("".into())).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn deserialize_optional_decimal_rejects_more_than_4dp() {
        let result: Result<Option<Decimal>, _> =
            deserialize_optional_decimal(serde_json::Value::String("1.23456".into()));
        assert!(result.is_err());
    }
}
