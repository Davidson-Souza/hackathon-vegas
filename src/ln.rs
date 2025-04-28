use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use bitcoin::hashes::Hash;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    amount: u64,
    bolt11: String,
    pub payment_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvoiceStatus {
    Unpaid,
    Paid,
}

pub trait LnBackend {
    type Error;

    fn get_invoice(&self, amount: u64) -> Result<Invoice, Self::Error>;
    fn get_invoice_status(&self, hash: String) -> Result<InvoiceStatus, Self::Error>;
}

pub struct MockLnBackend {
    invoices: Arc<Mutex<HashMap<String, (Invoice, InvoiceStatus)>>>,
}

impl MockLnBackend {
    pub fn new() -> Self {
        Self {
            invoices: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl LnBackend for MockLnBackend {
    type Error = ();

    fn get_invoice(&self, amount: u64) -> Result<Invoice, Self::Error> {
        let payment_preimage = "mock_payment_preimage".to_string();
        let payment_hash = bitcoin::hashes::sha256d::Hash::hash(payment_preimage.as_bytes());
        let invoice = Invoice {
            amount,
            bolt11: "mock_bolt11".to_string(),
            payment_hash: payment_hash.to_string(),
        };

        let mut invoices = self.invoices.lock().unwrap();
        invoices.insert(
            payment_hash.to_string(),
            (invoice.clone(), InvoiceStatus::Unpaid),
        );

        Ok(invoice)
    }

    fn get_invoice_status(&self, hash: String) -> Result<InvoiceStatus, Self::Error> {
        Ok(InvoiceStatus::Paid)
    }
}
