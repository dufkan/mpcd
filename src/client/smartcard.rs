use crate::client::Client;
use pcsc::Card;
use log::{info, warn};
use std::convert::TryInto;
use p256::{PublicKey, AffinePoint, Scalar};
use p256::elliptic_curve::sec1::ToEncodedPoint;

pub struct SmartcardClient {
    card: Card,
    rapdu: [u8; pcsc::MAX_BUFFER_SIZE]
}

fn make_apdu(cla: u8, ins: u8, p1: u8, p2: u8, data: Option<&[u8]>) -> Vec<u8> {
    let mut apdu_buffer = vec![cla, ins, p1, p2];
    if let Some(data) = data {
        apdu_buffer.push(data.len() as u8);
        apdu_buffer.extend_from_slice(data);
    }
    apdu_buffer
}

fn select_applet(card: &mut Card, aid: &[u8]) -> Result<(), String> {
    let apdu = make_apdu(0x00, 0xa4, 0x04, 0x00, Some(aid));
    info!("Selecting applet {}", std::str::from_utf8(aid).unwrap());
    let mut rapdu_buf = [0; pcsc::MAX_BUFFER_SIZE];
    let rapdu = match card.transmit(&apdu, &mut rapdu_buf) {
        Ok(rapdu) => rapdu,
        Err(err) => return Err(err.to_string())
    };
    if rapdu != b"\x90\x00" {
        warn!("Selection failed: {:?}", rapdu);
        return Err(format!("Selection failed {:?}", rapdu));
    }
    info!("Selected successfully");
    Ok(())
}

impl SmartcardClient {
    pub fn new(mut card: Card) -> Result<SmartcardClient, String> {
        match select_applet(&mut card, b"mpcapplet") {
            Ok(_) => Ok(SmartcardClient { card, rapdu: [0; pcsc::MAX_BUFFER_SIZE] }),
            Err(e) => Err(e)
        }
    }

    fn send_apdu(&mut self, apdu: &[u8]) -> Result<(u16, &[u8]), String> {
        self.card.transmit(apdu, &mut self.rapdu).map(|bytes| {
            let (rest, code) = bytes.split_at(bytes.len() - std::mem::size_of::<u16>());
            (u16::from_be_bytes(code.try_into().unwrap()), rest)
        }).map_err(|x| x.to_string())
    }
}

impl Client for SmartcardClient {
    fn get_info(&mut self) -> Result<String, String> {
        let (_, resp) = self.send_apdu(b"\xc0\xf0\x00\x00")?;
        Ok(std::str::from_utf8(resp).map(String::from).unwrap())
    }

    fn get_identity_key(&mut self) -> Result<PublicKey, String> {
        let (_, resp) = self.send_apdu(b"\xc0\xf1\x00\x00")?;
        match PublicKey::from_sec1_bytes(resp) {
            Ok(identity_key) => Ok(identity_key),
            Err(_) => Err(String::from("Received invalid identity key"))
        }
    }

    fn keygen_initialize(&mut self, group_size: usize) -> Result<Vec<u8>, String> {
        let (_, resp) = self.send_apdu(&[0xc1, 0xc0, group_size as u8, 0x00])?;
        Ok(resp.to_vec())
    }

    fn keygen_reveal(&mut self, commitments: Vec<Vec<u8>>) -> Result<PublicKey, String> {
        for (idx, commitment) in commitments.iter().enumerate() {
            let mut data = vec![0xc1, 0xc1, idx as u8, 0x00];
            data.push(commitment.len() as u8);
            data.extend_from_slice(commitment);
            self.send_apdu(&data)?;
        }
        let (_, resp) = self.send_apdu(&[0xc1, 0xc2, 0x00, 0x00])?;
        match PublicKey::from_sec1_bytes(resp) {
            Ok(public_key) => Ok(public_key),
            Err(_) => Err(String::from("Received invalid public key"))
        }
    }

    fn keygen_finalize(&mut self, public_keys: Vec<PublicKey>) -> Result<PublicKey, String> {
        for (idx, public_key) in public_keys.iter().enumerate() {
            let public_key = public_key.to_encoded_point(false).as_bytes().to_vec();
            let mut data = vec![0xc1, 0xc3, idx as u8, 0x00];
            data.push(public_key.len() as u8);
            data.extend_from_slice(&public_key);
            self.send_apdu(&data);
        }
        let (_, resp) = self.send_apdu(&[0xc1, 0xc4, 0x00, 0x00])?;
        match PublicKey::from_sec1_bytes(resp) {
            Ok(public_key) => Ok(public_key),
            Err(_) => Err(String::from("Received invalid group key"))
        }
    }

    fn get_nonce(&mut self, _counter: u16) -> PublicKey {
        unimplemented!()
    }

    fn cache_nonce(&mut self, _counter: u16) -> Vec<u8> {
        unimplemented!()
    }

    fn reveal_nonce(&mut self, _counter: u16) -> Vec<u8> {
        unimplemented!()
    }

    fn sign(&mut self, _counter: u16, _nonce_point: AffinePoint, _message: [u8; 32]) -> Scalar {
        unimplemented!()
    }

    fn sign_reveal(&mut self, _counter: u16, _nonce_point: AffinePoint, _message: [u8; 32]) -> (Scalar, Vec<u8>) {
        unimplemented!()
    }
}