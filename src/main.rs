//! This project should be a simple control server for a network of lockers, that can be used by
//! anyone after a small bitcoin payment. The lockers will accept a JWT token that is signed by the
//! server. This JWT will allow the user to open the locker, both for storing things and for
//! retrieving things after a certain time.

use std::env;
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Path;
use axum::extract::State;
use axum::routing::post;
use axum::{http::Method, routing::get, Router};
use bitcoin::hashes::HashEngine;
use bitcoin::hex::DisplayHex;
use ln::LnBackend;
use ln::PhoenixdClient;
use secp256k1::{Keypair, Secp256k1};
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

/// This is the main entry point for the server. It will start a web server that will listen for
/// incoming requests and handle them. It will also handle the JWT token generation and validation.
struct Server<Ln: LnBackend> {
    /// The server will use this secret to sign the JWT tokens.
    keypair: Keypair,
    /// The server will use this database to store the lockers and their state.
    database: Arc<Mutex<sqlite::Connection>>,
    ln: Ln,
}

async fn get_locker(
    Path(locker_id): Path<i64>,
    state: State<Arc<Server<PhoenixdClient>>>,
) -> Result<Body, error::Error> {
    let lockers = state.list_lockers().await?;
    let locker = lockers
        .iter()
        .find(|l| l.id == locker_id)
        .ok_or(error::Error::NotFound)?;

    let body = serde_json::json!({
        "data": locker,
        "error": null,
    });

    Ok(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
}

/// Returns the available lockers and their state. This will be used to display the lockers to the
/// user.
async fn get_lockers(state: State<Arc<Server<PhoenixdClient>>>) -> Result<Body, error::Error> {
    let lockers = state.list_lockers().await?;
    let body = serde_json::json!({
        "data": lockers,
        "error": null,
    });

    Ok(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
}

async fn use_locker(
    Path(locker_id): Path<i64>,
    state: State<Arc<Server<PhoenixdClient>>>,
) -> Result<Body, error::Error> {
    let locker_state = state.get_locker_state(locker_id).await?;
    if locker_state != "available" {
        let body = serde_json::json!({
            "data": null,
            "error": "Locker is not available",
        });

        return Ok(axum::body::Body::from(serde_json::to_vec(&body).unwrap()));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    state
        .set_locker_state(locker_id, "in_use".to_string())
        .await?;

    state.set_locker_start_time(locker_id, now).await?;

    let signature = {
        let mut hasher = bitcoin::hashes::sha256::HashEngine::default();
        hasher.write_all(format!("{}{}", locker_id, now).as_bytes())?;

        let hash = hasher.midstate().0;
        let secp = secp256k1::Secp256k1::new();
        let signature = secp.sign_schnorr_no_aux_rand(&hash, &state.keypair);

        signature.to_byte_array().to_upper_hex_string()
    };

    let body = serde_json::json!({
        "data": {
            "locker_id": locker_id,
            "start_time": now,
            "signature": signature,
        },
        "error": null,
    });

    Ok(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
}

async fn pay_for_usage(
    Path(locker_id): Path<i64>,
    state: State<Arc<Server<PhoenixdClient>>>,
) -> Result<Body, error::Error> {
    let locker_state = state.get_locker_state(locker_id).await?;
    if locker_state != "in_use" {
        return Err(error::Error::BadRequest);
    }

    let start_time = state.get_locker_start_time(locker_id.clone()).await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let lease_time = now - start_time;

    let invoice = state
        .ln
        .get_invoice(lease_time)
        .map_err(|_| error::Error::Server)?;

    let database = state.database.lock().await;
    let query = format!("INSERT INTO pending_payments (amount, payment_hash, status, locker_id) VALUES ({}, '{}', 'pending', '{}')", lease_time, invoice.payment_hash, locker_id);
    database.execute(query)?;

    let body = serde_json::json!({
        "data": {
            "locker_id": locker_id,
            "lease_time": lease_time,
            "invoice": invoice,
        },
        "error": null,
    });

    Ok(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
}

/// This will return a signed receipt for the payment. This receipt will be used to unlock
/// the locker. The receipt will be signed by the server and will contain the locker id, and the
/// current timestamp. The client will use this receipt to unlock the locker.
async fn get_pament_receipt(
    Path(payment_hash): Path<String>,
    state: State<Arc<Server<PhoenixdClient>>>,
) -> Result<Body, error::Error> {
    let payment_status = state
        .ln
        .get_invoice_status(payment_hash.clone())
        .map_err(|_| error::Error::BadRequest)?;

    if payment_status != ln::InvoiceStatus::Paid {
        return Err(error::Error::BadRequest);
    }

    let PendingPayment { locker_id, .. } = state.get_payment(payment_hash.clone()).await?;

    let start_time = state.get_locker_start_time(locker_id.clone()).await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let signature = {
        let mut hasher = bitcoin::hashes::sha256::HashEngine::default();
        hasher.write_all(format!("{}{}", locker_id, now).as_bytes())?;

        let hash = hasher.midstate().0;
        let secp = secp256k1::Secp256k1::new();
        let signature = secp.sign_schnorr_no_aux_rand(&hash, &state.keypair);

        signature.to_byte_array().to_upper_hex_string()
    };

    let body = serde_json::json!({
        "locker_id": locker_id,
        "start_time": start_time,
        "signature": signature,
    });

    Ok(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
}

async fn update_locker_open(
    state: State<Arc<Server<PhoenixdClient>>>,
    body: axum::Json<UpdateLockerOpen>,
) -> Result<Body, error::Error> {
    let locker_id = body.locker_id;
    let signature = secp256k1::schnorr::Signature::from_str(&body.signature).map_err(|_| error::Error::BadRequest)?;
    let pk = state.get_locker_pk(locker_id).await?;
    
    let secp = secp256k1::Secp256k1::new();

    // hash the timestamp and locker_id, verify the signature over the hash
    let mut hasher = bitcoin::hashes::sha256::HashEngine::default();
    hasher.write_all(format!("{}{}", locker_id, body.timestamp).as_bytes())?;

    let hash = hasher.midstate().0;
    let pk = secp256k1::XOnlyPublicKey::from_str(&pk).map_err(|_| error::Error::BadRequest)?;
    secp.verify_schnorr(&signature, &hash, &pk).map_err(|_| error::Error::BadRequest)?;


    state.set_locker_state(locker_id, "available".to_string()).await?;
    Ok(axum::body::Body::from("Locker opened"))
}


#[derive(Debug, Clone, Deserialize, Serialize)]
struct UpdateLockerOpen {
    locker_id: i64,
    signature: String,
    timestamp: u64,
}

#[allow(dead_code)]
struct PendingPayment {
    amount: u64,
    payment_hash: String,
    status: String,
    locker_id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Locker {
    id: i64,
    state: String,
}

impl Server<PhoenixdClient> {
    pub async fn run(address: String, keypair: Keypair, database: sqlite::Connection, ln: PhoenixdClient) {
        let listener = match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => listener,
            Err(_) => {
                std::process::exit(-1);
            }
        };

        let router = Router::new()
            .route("/use_locker/{locker_id}", get(use_locker))
            .route("/pay_for_usage/{locker_id}", get(pay_for_usage))
            .route("/payment_receipt/{payment_hash}", get(get_pament_receipt))
            .route("/lockers", get(get_lockers))
            .route("/lockers/{locker_id}", get(get_locker))
            .route("/update_locker_open", post(update_locker_open))
            .layer(
                CorsLayer::new()
                    .allow_private_network(true)
                    .allow_methods([Method::POST, Method::HEAD]),
            )
            .with_state(Arc::new(Server {
                keypair,
                database: Arc::new(Mutex::new(database)),
                ln,
            }));

        axum::serve(listener, router)
            .await
            .expect("failed to start rpc server");
    }
    
    async fn get_locker_pk(
        &self,
        locker_id: i64,
    ) -> Result<String, error::Error> {
        let database = self.database.lock().await;
        let query = format!("SELECT pk FROM lockers WHERE id = '{}'", locker_id);
        let mut statement = database.prepare(query)?;

        let sqlite::State::Row = statement.next()? else {
            return Err(error::Error::NotFound);
        };

        let pk: String = statement.read(0)?;
        Ok(pk)
    }

    async fn get_payment(&self, payment_hash: String) -> Result<PendingPayment, error::Error> {
        let database = self.database.lock().await;
        let query = format!("SELECT amount, payment_hash, status, locker_id FROM pending_payments WHERE payment_hash = '{}'", payment_hash);
        let mut statement = database.prepare(query)?;

        let sqlite::State::Row = statement.next()? else {
            return Err(error::Error::NotFound);
        };

        let amount: u64 = statement.read::<i64, _>(0)? as u64;
        let payment_hash: String = statement.read(1)?;
        let status: String = statement.read(2)?;
        let locker_id: i64 = statement.read(3)?;

        Ok(PendingPayment {
            amount,
            payment_hash,
            status,
            locker_id,
        })
    }

    async fn get_locker_state(&self, locker_id: i64) -> Result<String, error::Error> {
        let database = self.database.lock().await;
        let query = format!("SELECT state FROM lockers WHERE id = '{}'", locker_id);
        let mut statement = database.prepare(query)?;

        let sqlite::State::Row = statement.next()? else {
            return Err(error::Error::NotFound);
        };

        let state = statement.read::<String, _>(0)?;
        Ok(state)
    }

    async fn set_locker_state(&self, locker_id: i64, state: String) -> Result<(), error::Error> {
        let database = self.database.lock().await;
        let query = format!(
            "UPDATE lockers SET state = '{}' WHERE id = '{}'",
            state, locker_id
        );
        database.execute(query)?;
        Ok(())
    }

    async fn get_locker_start_time(&self, locker_id: i64) -> Result<u64, error::Error> {
        let database = self.database.lock().await;
        let query = format!("SELECT start_time FROM lockers WHERE id = '{}'", locker_id);
        let mut statement = database.prepare(query)?;

        let sqlite::State::Row = statement.next()? else {
            return Err(error::Error::NotFound);
        };

        let start_time = statement.read::<i64, _>(0)? as u64;
        Ok(start_time)
    }

    async fn set_locker_start_time(
        &self,
        locker_id: i64,
        start_time: u64,
    ) -> Result<(), error::Error> {
        let database = self.database.lock().await;
        let query = format!(
            "UPDATE lockers SET start_time = {} WHERE id = '{}'",
            start_time, locker_id
        );
        database.execute(query)?;
        Ok(())
    }

    async fn list_lockers(&self) -> Result<Vec<Locker>, error::Error> {
        let database = self.database.lock().await;
        let query = "SELECT id, state FROM lockers";
        let mut statement = database.prepare(query)?;

        let mut lockers = Vec::new();
        while let sqlite::State::Row = statement.next()? {
            let id: i64 = statement.read(0)?;
            let state: String = statement.read(1)?;
            let locker = Locker { id, state };

            lockers.push(locker);
        }

        Ok(lockers)
    }
}

mod error;
mod ln;

#[tokio::main]
async fn main() { 
    let password = env::var("PASSWORD").expect("PASSWORD not set");

    let phoenix = PhoenixdClient::new(
        "http://127.0.0.1:9740".to_string(),
        base64::encode(format!(":{password}"))
    );

    let database = sqlite::open(":memory:").unwrap();
    database
        .execute("CREATE TABLE IF NOT EXISTS lockers (id INTEGER PRIMARY KEY AUTOINCREMENT, pk TEXT NOT NULL, state TEXT NOT NULL, start_time INTEGER NOT NULL)")
        .unwrap();

    // create the table pending payments
    database
        .execute("CREATE TABLE IF NOT EXISTS pending_payments (id INTEGER PRIMARY KEY AUTOINCREMENT, amount INTEGER NOT NULL, payment_hash TEXT NOT NULL, status TEXT NOT NULL, locker_id TEXT NOT NULL, FOREIGN KEY (locker_id) REFERENCES lockers(id))")
        .unwrap();

    // add two lockers to the database
    database
        .execute("INSERT OR IGNORE INTO lockers (state, start_time, pk) VALUES ('available', 0, '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798')")
        .unwrap();

    database
        .execute("INSERT OR IGNORE INTO lockers (state, start_time, pk) VALUES ('available', 0, '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798')")
        .unwrap();

    let keypair = Keypair::from_seckey_str(
        &Secp256k1::default(),
        "0000000000000000000000000000000000000000000000000000000000000001",
    )
    .expect("failed to create keypair");

    println!("[+] Keypair created");
    println!("[+] Server pubkey: {}", keypair.x_only_public_key().0.to_string());
    println!("[+] Database created");
    println!("[+] Phoenix client created");
    println!("[+] Starting server...");
    // create the server
    Server::run("0.0.0.0:8080".to_string(), keypair, database, phoenix).await;
}
