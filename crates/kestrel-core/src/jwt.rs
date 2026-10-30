use anyhow::{Result, anyhow, bail};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::Value as Json;

pub(crate) fn sign_for_script(header: &str, claims: &str, key: &str) -> String {
    match sign(header, claims, key) {
        Ok(token) => serde_json::json!({ "ok": token }).to_string(),
        Err(error) => serde_json::json!({ "err": format!("jwt.sign: {error}") }).to_string(),
    }
}

fn sign(header: &str, claims: &str, key: &str) -> Result<String> {
    let header_json: Json = serde_json::from_str(header)?;
    let claims: Json = serde_json::from_str(claims)?;
    let algorithm_name = header_json.get("alg").and_then(Json::as_str).unwrap_or("HS256");
    let algorithm: Algorithm = algorithm_name.parse().map_err(|_| anyhow!("unknown algorithm {algorithm_name}"))?;
    let encoding = encoding_key(algorithm, &key.replace("\\n", "\n"))?;
    let mut jwt_header = Header::new(algorithm);
    jwt_header.kid = header_json.get("kid").and_then(Json::as_str).map(str::to_string);
    jwt_header.typ = header_json.get("typ").and_then(Json::as_str).map(str::to_string);
    Ok(jsonwebtoken::encode(&jwt_header, &claims, &encoding)?)
}

fn encoding_key(algorithm: Algorithm, key: &str) -> Result<EncodingKey> {
    let pem = key.as_bytes();
    Ok(match algorithm {
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => EncodingKey::from_secret(pem),
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 | Algorithm::PS256 | Algorithm::PS384 | Algorithm::PS512 => {
            EncodingKey::from_rsa_pem(pem)?
        }
        Algorithm::ES256 | Algorithm::ES384 => EncodingKey::from_ec_pem(pem)?,
        Algorithm::EdDSA => EncodingKey::from_ed_pem(pem)?,
        other => bail!("algorithm {other:?} is not supported"),
    })
}
