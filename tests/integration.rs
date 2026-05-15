use payments_engine::engine::PaymentEngine;
use payments_engine::models::TransactionRow;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::io::Write;
use tempfile::NamedTempFile;

fn run_engine(csv_content: &str) -> String {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", csv_content).unwrap();

    let mut engine = PaymentEngine::new();
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(tmp.path())
        .unwrap();

    for result in reader.deserialize() {
        let row: TransactionRow = result.unwrap();
        engine.process_transaction(row);
    }

    let mut buf = Vec::new();
    {
        let mut writer = csv::Writer::from_writer(&mut buf);
        for client in engine.clients_sorted() {
            writer.serialize(client).unwrap();
        }
        writer.flush().unwrap();
    }
    String::from_utf8(buf).unwrap()
}

fn parse_output(output: &str) -> HashMap<u16, (Decimal, Decimal, Decimal, bool)> {
    let mut map = HashMap::new();
    let mut reader = csv::ReaderBuilder::new().from_reader(output.as_bytes());
    for row in reader.records() {
        let r = row.unwrap();
        let id: u16 = r[0].parse().unwrap();
        let available: Decimal = r[1].parse().unwrap();
        let held: Decimal = r[2].parse().unwrap();
        let total: Decimal = r[3].parse().unwrap();
        let locked: bool = r[4].parse().unwrap();
        map.insert(id, (available, held, total, locked));
    }
    map
}

fn dec(s: &str) -> Decimal {
    s.parse().unwrap()
}

#[test]
fn basic_deposit_and_withdrawal() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,1.0\n\
         deposit,2,2,2.0\n\
         deposit,1,3,2.0\n\
         withdrawal,1,4,1.5\n\
         withdrawal,2,5,3.0\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (dec("1.5000"), Decimal::ZERO, dec("1.5000"), false)
    );
    assert_eq!(
        clients[&2],
        (dec("2.0000"), Decimal::ZERO, dec("2.0000"), false)
    );
}

#[test]
fn dispute_resolve_flow() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,1,1,\n\
         resolve,1,1,\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (dec("100.0000"), Decimal::ZERO, dec("100.0000"), false)
    );
}

#[test]
fn dispute_chargeback_flow() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,1,1,\n\
         chargeback,1,1,\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, true)
    );
}

#[test]
fn dispute_on_withdrawal() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,30.0\n\
         dispute,1,2,\n\
         resolve,1,2,\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (dec("70.0000"), Decimal::ZERO, dec("70.0000"), false)
    );
}

#[test]
fn chargeback_freezes_account() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,1,2,50.0\n\
         dispute,1,1,\n\
         chargeback,1,1,\n\
         deposit,1,3,25.0\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (dec("50.0000"), Decimal::ZERO, dec("50.0000"), true)
    );
}

#[test]
fn four_decimal_precision_roundtrip() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,1.2345\n\
         withdrawal,1,2,0.0001\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (dec("1.2344"), Decimal::ZERO, dec("1.2344"), false)
    );
}

#[test]
fn whitespace_handling() {
    let output = run_engine(
        "type, client, tx, amount\n\
         deposit, 1, 1, 5.5\n\
         withdrawal, 1, 2, 1.5\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (dec("4.0000"), Decimal::ZERO, dec("4.0000"), false)
    );
}

#[test]
fn unknown_transaction_type_ignored() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         foobar,1,2,50.0\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (dec("100.0000"), Decimal::ZERO, dec("100.0000"), false)
    );
}

#[test]
fn empty_csv_produces_header_only() {
    let output = run_engine("type,client,tx,amount\n");
    let clients = parse_output(&output);
    assert!(clients.is_empty());
}

#[test]
fn unordered_transaction_ids() {
    let output = run_engine(
        "type,client,tx,amount\n\
         withdrawal,1,5,1.0\n\
         deposit,1,3,5.0\n\
         withdrawal,1,1,2.0\n\
         deposit,1,2,3.0\n",
    );
    let clients = parse_output(&output);
    assert_eq!(
        clients[&1],
        (dec("6.0000"), Decimal::ZERO, dec("6.0000"), false)
    );
}

#[test]
fn chargeback_on_disputed_withdrawal() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,30.0\n\
         dispute,1,2,\n\
         chargeback,1,2,\n",
    );
    let clients = parse_output(&output);
    // Withdrawal was disputed (available restored to 100, held = 30, total = 100)
    // Chargeback: held -= 30, total -= 30, locked
    // Result: available = 100, held = 0, total = 70, locked = true
    assert_eq!(
        clients[&1],
        (dec("100.0000"), Decimal::ZERO, dec("70.0000"), true)
    );
}

#[test]
fn double_dispute_ignored() {
    let output = run_engine(
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,1,1,\n\
         dispute,1,1,\n",
    );
    let clients = parse_output(&output);
    // Second dispute should be ignored — tx already under_dispute
    assert_eq!(
        clients[&1],
        (Decimal::ZERO, dec("100.0000"), dec("100.0000"), false)
    );
}
