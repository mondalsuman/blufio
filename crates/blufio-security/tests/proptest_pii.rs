// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Property-based tests for PII detection.
//!
//! Validates that the PII detection engine correctly identifies common PII
//! patterns and avoids false positives with randomized inputs.

use blufio_security::pii::{PiiType, detect_pii, luhn_validate};
use proptest::prelude::*;

/// Strategy to generate valid email addresses (user@domain.tld).
fn email_strategy() -> impl Strategy<Value = String> {
    (
        "[a-z][a-z0-9._%+-]{1,15}", // local part
        "[a-z][a-z0-9-]{1,10}",     // domain
        prop::sample::select(vec!["com", "org", "net", "io", "co.uk"]),
    )
        .prop_map(|(user, domain, tld)| format!("{user}@{domain}.{tld}"))
}

/// Strategy to generate valid US phone numbers in various formats.
fn us_phone_strategy() -> impl Strategy<Value = String> {
    (
        prop::sample::select(vec!["", "+1 ", "+1-", "1-"]),
        "[2-9][0-9]{2}", // area code (cannot start with 0 or 1)
        "[2-9][0-9]{2}", // exchange
        "[0-9]{4}",      // subscriber
    )
        .prop_map(|(prefix, area, exchange, sub)| format!("{prefix}{area}-{exchange}-{sub}"))
}

/// Strategy to generate valid SSN patterns (NNN-NN-NNNN).
fn ssn_strategy() -> impl Strategy<Value = String> {
    (
        // Area: 001-665, 667-899 (valid range excluding 000, 666, 900+)
        prop::sample::select((1u16..=665).chain(667..=899).collect::<Vec<_>>()),
        10u8..=99,      // group
        1000u16..=9999, // serial
    )
        .prop_map(|(area, group, serial)| format!("{area:03}-{group:02}-{serial:04}"))
}

/// Strategy to generate Luhn-valid credit card numbers.
///
/// Generates a 15-digit prefix, computes the Luhn check digit, appends it.
fn credit_card_strategy() -> impl Strategy<Value = String> {
    (
        // Start with valid BIN prefix (4=Visa, 5=MC, 3=Amex, 6=Discover)
        prop::sample::select(vec![4u32, 5, 3, 6]),
        proptest::collection::vec(0u32..10, 14), // 14 more digits
    )
        .prop_map(|(prefix, mut digits)| {
            digits.insert(0, prefix);
            // Compute Luhn check digit
            let check = compute_luhn_check_digit(&digits);
            digits.push(check);
            digits
                .iter()
                .map(|d| char::from_digit(*d, 10).unwrap())
                .collect::<String>()
        })
}

/// Compute the Luhn check digit for a sequence of digits.
fn compute_luhn_check_digit(digits: &[u32]) -> u32 {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            // For check digit computation, even positions (0-indexed from right) get doubled
            if i % 2 == 0 {
                let doubled = d * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                d
            }
        })
        .sum();
    (10 - (sum % 10)) % 10
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..Default::default() })]

    // ── Property 1: valid emails are always detected ──────────────────

    #[test]
    fn generated_valid_emails_detected(email in email_strategy()) {
        let text = format!("Contact: {email} for info");
        let matches = detect_pii(&text);
        let email_matches: Vec<_> = matches.iter().filter(|m| m.pii_type == PiiType::Email).collect();
        prop_assert!(
            !email_matches.is_empty(),
            "Generated email '{email}' should be detected as PII"
        );
    }

    // ── Property 2: valid US phone numbers are detected ───────────────

    #[test]
    fn generated_valid_phones_detected(phone in us_phone_strategy()) {
        let text = format!("Call {phone} now");
        let matches = detect_pii(&text);
        let phone_matches: Vec<_> = matches.iter().filter(|m| m.pii_type == PiiType::Phone).collect();
        prop_assert!(
            !phone_matches.is_empty(),
            "Generated phone '{phone}' should be detected as PII"
        );
    }

    // ── Property 3: valid SSN patterns are detected ───────────────────

    #[test]
    fn generated_valid_ssns_detected(ssn in ssn_strategy()) {
        let text = format!("SSN: {ssn}");
        let matches = detect_pii(&text);
        let ssn_matches: Vec<_> = matches.iter().filter(|m| m.pii_type == PiiType::Ssn).collect();
        prop_assert!(
            !ssn_matches.is_empty(),
            "Generated SSN '{ssn}' should be detected as PII"
        );
    }

    // ── Property 4: Luhn-valid credit card numbers are detected ───────

    #[test]
    fn generated_valid_credit_cards_detected(cc in credit_card_strategy()) {
        // First verify our generator produces valid Luhn numbers
        prop_assert!(luhn_validate(&cc), "Generated CC '{cc}' should pass Luhn");

        let text = format!("Card: {cc}");
        let matches = detect_pii(&text);
        let cc_matches: Vec<_> = matches.iter().filter(|m| m.pii_type == PiiType::CreditCard).collect();
        prop_assert!(
            !cc_matches.is_empty(),
            "Generated Luhn-valid CC '{cc}' should be detected as PII"
        );
    }

    // ── Property 5: random alphanumeric strings produce no false positives ──

    #[test]
    fn random_alphanumeric_no_false_positives(
        text in "[a-zA-Z ]{20,100}"
    ) {
        // Pure alphabetic strings with spaces should never contain PII
        let matches = detect_pii(&text);
        prop_assert!(
            matches.is_empty(),
            "Pure alphabetic text should not trigger PII detection. Text: '{}', matches: {:?}",
            &text[..text.len().min(50)],
            matches.iter().map(|m| format!("{}: '{}'", m.pii_type, &m.matched_value)).collect::<Vec<_>>()
        );
    }

    // ── Property 6: Luhn check digit correctness ──────────────────────

    #[test]
    fn luhn_generated_numbers_always_valid(
        prefix_digit in prop::sample::select(vec![4u32, 5, 3, 6]),
        middle_digits in proptest::collection::vec(0u32..10, 14),
    ) {
        let mut digits = vec![prefix_digit];
        digits.extend_from_slice(&middle_digits);
        let check = compute_luhn_check_digit(&digits);
        digits.push(check);
        let number: String = digits.iter().map(|d| char::from_digit(*d, 10).unwrap()).collect();
        prop_assert!(
            luhn_validate(&number),
            "Generated number with computed check digit should always pass Luhn: {number}"
        );
    }
}
