use crate::models::{Client, TransactionKind, TransactionRecord, TransactionRow};
use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Default)]
pub struct PaymentEngine {
    clients: HashMap<u16, Client>,
    transactions: HashMap<u32, TransactionRecord>,
}

impl PaymentEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process_transaction(&mut self, row: TransactionRow) {
        if self.is_locked(row.client) {
            return;
        }
        match row.tx_type.as_str() {
            "deposit" => {
                if let Some(amount) = row.amount {
                    self.deposit(row.client, row.tx, amount);
                }
            }
            "withdrawal" => {
                if let Some(amount) = row.amount {
                    self.withdrawal(row.client, row.tx, amount);
                }
            }
            "dispute" => self.dispute(row.client, row.tx),
            "resolve" => self.resolve(row.client, row.tx),
            "chargeback" => self.chargeback(row.client, row.tx),
            _ => {}
        }
    }

    fn is_locked(&self, client_id: u16) -> bool {
        self.clients.get(&client_id).map_or(false, |c| c.locked)
    }

    fn get_or_create_client(&mut self, client_id: u16) -> &mut Client {
        self.clients
            .entry(client_id)
            .or_insert_with(|| Client::new(client_id))
    }

    // DRY: shared validation for dispute/resolve/chargeback.
    // Returns Copy fields — avoids cloning TransactionRecord (anti-pattern).
    fn lookup_tx(
        &self,
        client_id: u16,
        tx_id: u32,
        under_dispute: bool,
    ) -> Option<(TransactionKind, Decimal)> {
        let tx = self.transactions.get(&tx_id)?;
        if tx.client_id != client_id || tx.under_dispute != under_dispute {
            return None;
        }
        Some((tx.kind, tx.amount))
    }

    fn deposit(&mut self, client_id: u16, tx_id: u32, amount: Decimal) {
        if self.is_locked(client_id) {
            return;
        }
        let client = self.get_or_create_client(client_id);
        client.available += amount;
        client.total += amount;
        self.transactions.insert(
            tx_id,
            TransactionRecord {
                kind: TransactionKind::Deposit,
                client_id,
                amount,
                under_dispute: false,
            },
        );
    }

    fn withdrawal(&mut self, client_id: u16, tx_id: u32, amount: Decimal) {
        if self.is_locked(client_id) {
            return;
        }
        let client = self.get_or_create_client(client_id);
        if client.available < amount {
            return;
        }
        client.available -= amount;
        client.total -= amount;
        self.transactions.insert(
            tx_id,
            TransactionRecord {
                kind: TransactionKind::Withdrawal,
                client_id,
                amount,
                under_dispute: false,
            },
        );
    }

    fn dispute(&mut self, client_id: u16, tx_id: u32) {
        if self.is_locked(client_id) {
            return;
        }
        let (kind, amount) = match self.lookup_tx(client_id, tx_id, false) {
            Some(v) => v,
            None => return,
        };
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.apply_dispute(kind, amount);
            self.transactions.get_mut(&tx_id).unwrap().under_dispute = true;
        }
    }

    fn resolve(&mut self, client_id: u16, tx_id: u32) {
        if self.is_locked(client_id) {
            return;
        }
        let (kind, amount) = match self.lookup_tx(client_id, tx_id, true) {
            Some(v) => v,
            None => return,
        };
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.apply_resolve(kind, amount);
            self.transactions.get_mut(&tx_id).unwrap().under_dispute = false;
        }
    }

    fn chargeback(&mut self, client_id: u16, tx_id: u32) {
        if self.is_locked(client_id) {
            return;
        }
        let (_kind, amount) = match self.lookup_tx(client_id, tx_id, true) {
            Some(v) => v,
            None => return,
        };
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.held -= amount;
            client.total -= amount;
            client.locked = true;
            self.transactions.get_mut(&tx_id).unwrap().under_dispute = false;
        }
    }

    pub fn clients_sorted(&self) -> Vec<&Client> {
        let mut clients: Vec<_> = self.clients.values().collect();
        clients.sort_by_key(|c| c.id);
        clients
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn deposit_increases_available_and_total() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("100.0"));
        assert_eq!(c.total, dec("100.0"));
        assert_eq!(c.held, Decimal::ZERO);
    }

    #[test]
    fn withdrawal_decreases_available_and_total() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.withdrawal(1, 2, dec("30.0"));
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("70.0"));
        assert_eq!(c.total, dec("70.0"));
    }

    #[test]
    fn withdrawal_insufficient_funds_is_ignored() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("10.0"));
        engine.withdrawal(1, 2, dec("50.0"));
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("10.0"));
        assert_eq!(c.total, dec("10.0"));
        assert!(!engine.transactions.contains_key(&2));
    }

    #[test]
    fn dispute_on_deposit_moves_funds_to_held() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(1, 1);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, Decimal::ZERO);
        assert_eq!(c.held, dec("100.0"));
        assert_eq!(c.total, dec("100.0"));
    }

    #[test]
    fn dispute_on_withdrawal_returns_funds_to_held() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.withdrawal(1, 2, dec("30.0"));
        engine.dispute(1, 2);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("100.0"));
        assert_eq!(c.held, dec("30.0"));
        assert_eq!(c.total, dec("100.0"));
    }

    #[test]
    fn resolve_on_deposit_returns_held_to_available() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(1, 1);
        engine.resolve(1, 1);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("100.0"));
        assert_eq!(c.held, Decimal::ZERO);
        assert_eq!(c.total, dec("100.0"));
    }

    #[test]
    fn chargeback_removes_held_and_total_and_locks() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(1, 1);
        engine.chargeback(1, 1);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, Decimal::ZERO);
        assert_eq!(c.held, Decimal::ZERO);
        assert_eq!(c.total, Decimal::ZERO);
        assert!(c.locked);
    }

    #[test]
    fn locked_account_ignores_all_transactions() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(1, 1);
        engine.chargeback(1, 1);
        engine.deposit(1, 2, dec("50.0"));
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.total, Decimal::ZERO);
        assert!(c.locked);
    }

    #[test]
    fn dispute_nonexistent_tx_ignored() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(1, 999);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("100.0"));
        assert_eq!(c.held, Decimal::ZERO);
    }

    #[test]
    fn resolve_non_disputed_tx_ignored() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.resolve(1, 1);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("100.0"));
        assert_eq!(c.held, Decimal::ZERO);
    }

    #[test]
    fn dispute_wrong_client_ignored() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(2, 1);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("100.0"));
        assert_eq!(c.held, Decimal::ZERO);
    }

    #[test]
    fn multiple_clients_independent() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.deposit(2, 2, dec("200.0"));
        engine.withdrawal(2, 3, dec("50.0"));
        let clients = engine.clients_sorted();
        assert_eq!(clients[0].id, 1);
        assert_eq!(clients[0].total, dec("100.0"));
        assert_eq!(clients[1].id, 2);
        assert_eq!(clients[1].total, dec("150.0"));
    }

    #[test]
    fn four_decimal_precision() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("1.2345"));
        engine.withdrawal(1, 2, dec("0.0010"));
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("1.2335"));
    }

    #[test]
    fn default_creates_empty_engine() {
        let engine = PaymentEngine::default();
        assert!(engine.clients_sorted().is_empty());
    }

    // --- Boundary tests ---

    #[test]
    fn withdrawal_of_exact_available_succeeds() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("50.0"));
        engine.withdrawal(1, 2, dec("50.0"));
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, Decimal::ZERO);
        assert_eq!(c.total, Decimal::ZERO);
    }

    #[test]
    fn successful_withdrawal_stored_in_transactions() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.withdrawal(1, 2, dec("30.0"));
        assert!(engine.transactions.contains_key(&2));
    }

    // --- Contract / Invariant tests (Cap 4) ---

    #[test]
    fn invariant_total_equals_available_plus_held_after_deposit() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.total, c.available + c.held);
    }

    #[test]
    fn invariant_total_equals_available_plus_held_after_dispute() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.withdrawal(1, 2, dec("30.0"));
        engine.dispute(1, 1);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.total, c.available + c.held);
    }

    #[test]
    fn invariant_total_equals_available_plus_held_after_chargeback() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(1, 1);
        engine.chargeback(1, 1);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.total, c.available + c.held);
    }

    // --- Chargeback guard tests ---

    #[test]
    fn chargeback_non_disputed_tx_ignored() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.chargeback(1, 1); // not under dispute
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("100.0"));
        assert!(!c.locked);
    }

    #[test]
    fn chargeback_nonexistent_tx_ignored() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.chargeback(1, 999);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.available, dec("100.0"));
        assert!(!c.locked);
    }

    // --- State transition guard tests ---

    #[test]
    fn resolve_after_chargeback_ignored() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(1, 1);
        engine.chargeback(1, 1);
        engine.resolve(1, 1); // tx no longer disputed
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert_eq!(c.held, Decimal::ZERO);
        assert!(c.locked); // still locked
    }

    #[test]
    fn locked_is_permanent_after_chargeback() {
        let mut engine = PaymentEngine::new();
        engine.deposit(1, 1, dec("100.0"));
        engine.dispute(1, 1);
        engine.chargeback(1, 1);
        engine.resolve(1, 1);
        engine.dispute(1, 1);
        let c = engine.clients_sorted().into_iter().find(|c| c.id == 1).unwrap();
        assert!(c.locked);
    }

    // --- Auto-create client ---

    #[test]
    fn deposit_creates_client_automatically() {
        let mut engine = PaymentEngine::new();
        engine.deposit(42, 1, dec("10.0"));
        assert!(engine.clients_sorted().iter().any(|c| c.id == 42));
    }
}
