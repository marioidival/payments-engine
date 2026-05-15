use payments_engine::engine::PaymentEngine;
use payments_engine::models::TransactionRow;
use std::env;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: payments-engine <transactions.csv>");
        std::process::exit(1);
    }

    let mut engine = PaymentEngine::new();
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(&args[1])?;

    for result in reader.deserialize() {
        let row: TransactionRow = result?;
        engine.process_transaction(row);
    }

    let mut writer = csv::Writer::from_writer(std::io::stdout());
    for client in engine.clients_sorted() {
        writer.serialize(client)?;
    }
    writer.flush()?;

    Ok(())
}
