use aes::{
    cipher::{generic_array, BlockDecrypt, BlockEncrypt, KeyInit},
    Aes256,
};
use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProxyData {
    pub destination: String,
    pub query_params: Option<serde_json::Value>,
    pub request_headers: Option<serde_json::Value>,
    pub response_headers: Option<serde_json::Value>,
    pub exp: Option<u64>,
    pub ip: Option<String>,
}

#[derive(Clone)]
pub struct EncryptionHandler {
    cipher: Aes256,
}

impl EncryptionHandler {
    pub fn new(api_password: &[u8]) -> Result<Self> {
        let key = if api_password.len() < 32 {
            let mut padded = [0u8; 32];
            padded[..api_password.len()].copy_from_slice(api_password);
            padded
        } else {
            let mut truncated = [0u8; 32];
            truncated.copy_from_slice(&api_password[..32]);
            truncated
        };

        Ok(Self {
            cipher: Aes256::new(&key.into()),
        })
    }

    pub fn encrypt(&self, data: &ProxyData) -> Result<String, AppError> {
        // Serialize the data to JSON
        let json_data = serde_json::to_vec(data)
            .map_err(|e| AppError::Internal(format!("Failed to serialize proxy data: {}", e)))?;

        // Generate random IV
        let mut iv = [0u8; 16];
        rand::rng().fill_bytes(&mut iv);

        // Pad the data
        let padded_data = self.pad_data(&json_data);

        // Encrypt the data
        let mut encrypted_data = padded_data.clone();
        for chunk in encrypted_data.chunks_mut(16) {
            let block = generic_array::GenericArray::from_mut_slice(chunk);
            self.cipher.encrypt_block(block);
        }

        // Combine IV and encrypted data
        let mut final_data = Vec::with_capacity(iv.len() + encrypted_data.len());
        final_data.extend_from_slice(&iv);
        final_data.extend_from_slice(&encrypted_data);

        // Encode to base64
        Ok(URL_SAFE.encode(final_data))
    }

    pub fn decrypt(&self, token: &str, client_ip: Option<&str>) -> Result<ProxyData, AppError> {
        // Decode base64
        let encrypted_data = URL_SAFE
            .decode(token)
            .map_err(|e| AppError::Auth(format!("Invalid token format: {}", e)))?;

        if encrypted_data.len() < 16 {
            return Err(AppError::Auth("Token too short".to_string()));
        }

        // Split IV and data
        let (_iv, encrypted) = encrypted_data.split_at(16);
        let mut decrypted = encrypted.to_vec();

        // Decrypt the data
        for chunk in decrypted.chunks_mut(16) {
            let block = generic_array::GenericArray::from_mut_slice(chunk);
            self.cipher.decrypt_block(block);
        }

        // Unpad the data
        let unpadded_data = self
            .unpad_data(&decrypted)
            .map_err(|_| AppError::Auth("Invalid padding".to_string()))?;

        // Deserialize the JSON data
        let proxy_data: ProxyData = serde_json::from_slice(&unpadded_data)
            .map_err(|e| AppError::Auth(format!("Invalid token data: {}", e)))?;

        // Validate expiration if set
        if let Some(exp) = proxy_data.exp {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if exp < now {
                return Err(AppError::Auth("Token has expired".to_string()));
            }
        }

        // Validate IP if both token IP and client IP are present
        if let (Some(token_ip), Some(client_ip)) = (proxy_data.ip.as_ref(), client_ip) {
            if token_ip != client_ip {
                return Err(AppError::Auth("IP mismatch".to_string()));
            }
        }

        Ok(proxy_data)
    }

    fn pad_data(&self, data: &[u8]) -> Vec<u8> {
        let block_size = 16;
        let padding_len = block_size - (data.len() % block_size);
        let mut padded = Vec::with_capacity(data.len() + padding_len);
        padded.extend_from_slice(data);
        padded.extend(std::iter::repeat(padding_len as u8).take(padding_len));
        padded
    }

    fn unpad_data(&self, data: &[u8]) -> Result<Vec<u8>, ()> {
        if data.is_empty() {
            return Err(());
        }

        let padding_len = *data.last().ok_or(())? as usize;
        if padding_len == 0 || padding_len > 16 || padding_len > data.len() {
            return Err(());
        }

        let unpadded_len = data.len() - padding_len;
        if !data[unpadded_len..].iter().all(|&x| x == padding_len as u8) {
            return Err(());
        }

        Ok(data[..unpadded_len].to_vec())
    }
}
