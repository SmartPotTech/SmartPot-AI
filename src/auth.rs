//! Token de servicio compartido con la API, comparado en tiempo constante.

use subtle::ConstantTimeEq;

pub const INVALID_TOKEN: &str = "Token de servicio inválido";

/// `true` si la cabecera es `Bearer <token>` (esquema sin distinguir mayúsculas) con el token esperado.
pub fn bearer_matches(header: Option<&str>, expected: &str) -> bool {
    let Some(header) = header else { return false };
    let Some((scheme, credentials)) = header.trim().split_once(' ') else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return false;
    }
    let credentials = credentials.trim().as_bytes();
    let expected = expected.as_bytes();
    // La longitud no es secreta (el token tiene un mínimo conocido); el contenido se compara en tiempo constante.
    credentials.len() == expected.len() && bool::from(credentials.ct_eq(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_scheme_and_token() {
        let token = "demo-ai-token-for-local-use-only";
        assert!(bearer_matches(
            Some("Bearer demo-ai-token-for-local-use-only"),
            token
        ));
        assert!(bearer_matches(
            Some("bearer demo-ai-token-for-local-use-only"),
            token
        ));
        assert!(!bearer_matches(
            Some("Basic demo-ai-token-for-local-use-only"),
            token
        ));
        assert!(!bearer_matches(
            Some("Bearer demo-ai-token-for-local-use-onlx"),
            token
        ));
        assert!(!bearer_matches(Some("Bearer"), token));
        assert!(!bearer_matches(None, token));
    }
}
