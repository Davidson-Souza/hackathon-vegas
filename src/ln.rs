use std::{
    collections::HashMap, fmt::Display, sync::{Arc, Mutex}
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

#[derive(Clone)]
/// A struct that holds all data needed to connect with a running phoenixd,
/// the actual lightning wallet powering this application
pub struct PhoenixdClient {
    /// The password we use to authenticate with phoenixd.
    ///
    /// You can find this in $PHOENIXD_DATA_DIR/phoenixd.conf
    pub password: String,

    /// The host where phoenixd is running
    pub host: String,
}

#[derive(Default, Serialize, Deserialize)]
#[allow(non_snake_case)]
/// Data returned from phoenixd when we call "getInvoice"
///
/// The most importanti info here is the "serialized" field, that contains the bolt11
/// invoice
pub struct GetInvoiceResponse {
    #[serde(rename = "type")]
    invoice_type: String,
    subType: String,
    paymentHash: String,
    preimage: String,
    externalId: Option<String>,
    description: String,
    invoice: String,
    isPaid: bool,
    receivedSat: u64,
    fees: u64,
    completedAt: Option<u64>,
    createdAt: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[allow(non_snake_case)]
/// Data returned from phoenixd when we call "createinvoice"
pub struct CreateInvoiceResponse {
    /// The payment hash for this invoice.
    pub paymentHash: String,

    /// The actual bolt11 invoice
    pub serialized: String,
}

impl PhoenixdClient {
    /// Create a new PhoenixdClient
    ///
    /// # Arguments
    ///
    /// * `host` - The host where phoenixd is running
    /// * `password` - The password we use to authenticate with phoenixd.
    ///
    /// You can find this in $PHOENIXD_DATA_DIR/phoenixd.conf
    pub fn new(host: String, password: String) -> Self {
        Self { password, host }
    }
}

#[derive(Debug)]
pub enum PhoenixdError {
    SerdeJson(serde_json::Error),
    MinReqHttp(minreq::Error),
}

impl Display for PhoenixdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhoenixdError::SerdeJson(err) => write!(f, "SerdeJson error: {}", err),
            PhoenixdError::MinReqHttp(err) => write!(f, "MinReqHttp error: {}", err),
        }
    }
}

impl From<minreq::Error> for PhoenixdError {
    fn from(err: minreq::Error) -> Self {
        PhoenixdError::MinReqHttp(err)
    }
}

impl From<serde_json::Error> for PhoenixdError {
    fn from(err: serde_json::Error) -> Self {
        PhoenixdError::SerdeJson(err)
    }
}

impl LnBackend for PhoenixdClient {
    type Error = PhoenixdError;

    fn get_invoice(&self, amount: u64) -> Result<Invoice, Self::Error> {
        let url = format!("{}/createinvoice", self.host);
        let response = minreq::post(url).with_body(
            format!(
                "\rdescription=Test invoice&amount={amount}&expirySeconds=3600",
            )
        )
        .with_header("Content-Type", "application/x-www-form-urlencoded")
        .with_header("Authorization", format!("Basic {}", self.password.clone()))
        .send()?;

        println!("[get_invoice] response: {:?}", response.as_str().unwrap());
        let response: CreateInvoiceResponse = serde_json::from_str(response.as_str()?)?;
        Ok(Invoice {
            amount,
            bolt11: response.serialized,
            payment_hash: response.paymentHash,
        })
    }

    fn get_invoice_status(&self, hash: String) -> Result<InvoiceStatus, Self::Error> {
        let url = format!("{}//payments/incoming/{}", self.host, hash);
        let response = minreq::get(url)
            .with_header("Authorization", format!("Basic {}", self.password.clone()))
            .send()?;
        
        println!("[get_invoice_status] response: {:?}", response.as_str().unwrap());
        let response: GetInvoiceResponse = serde_json::from_str(response.as_str()?)?;
        Ok(if response.isPaid {
            InvoiceStatus::Paid
        } else {
            InvoiceStatus::Unpaid
        })
    }
}
