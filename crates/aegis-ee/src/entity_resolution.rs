//! Probabilistic Entity Resolution and Consortium Graph Linkage.
//!
//! Links disparate accounts and customers across institutions using fuzzy identity
//! matching (Jaro-Winkler) and multi-token graph overlap (device, IP, tax hashes).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityCluster {
    pub cluster_id: String,
    pub primary_name: String,
    pub accounts: Vec<String>,
    pub confidence_score: f64,
    pub linked_by: Vec<String>,
}

/// Compute Jaro-Winkler string similarity in [0.0, 1.0].
pub fn jaro_winkler(s1: &str, s2: &str) -> f64 {
    let s1 = s1.trim().to_lowercase();
    let s2 = s2.trim().to_lowercase();

    if s1 == s2 {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }

    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    let match_distance = (len1.max(len2) / 2).saturating_sub(1);

    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();

    let mut s1_matches = vec![false; len1];
    let mut s2_matches = vec![false; len2];

    let mut matches = 0;
    for i in 0..len1 {
        let start = i.saturating_sub(match_distance);
        let end = (i + match_distance + 1).min(len2);
        for j in start..end {
            if s2_matches[j] || s1_chars[i] != s2_chars[j] {
                continue;
            }
            s1_matches[i] = true;
            s2_matches[j] = true;
            matches += 1;
            break;
        }
    }

    if matches == 0 {
        return 0.0;
    }

    let mut transpositions = 0;
    let mut k = 0;
    for i in 0..len1 {
        if !s1_matches[i] {
            continue;
        }
        while !s2_matches[k] {
            k += 1;
        }
        if s1_chars[i] != s2_chars[k] {
            transpositions += 1;
        }
        k += 1;
    }

    let m = matches as f64;
    let jaro = (m / len1 as f64 + m / len2 as f64 + (m - transpositions as f64 / 2.0) / m) / 3.0;

    // Common prefix up to 4 chars
    let mut prefix = 0;
    for (c1, c2) in s1_chars.iter().zip(s2_chars.iter()) {
        if c1 == c2 && prefix < 4 {
            prefix += 1;
        } else {
            break;
        }
    }

    let p = 0.1; // scaling factor
    jaro + (prefix as f64 * p * (1.0 - jaro))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jaro_winkler_exact_and_typos() {
        assert_eq!(jaro_winkler("John Doe", "John Doe"), 1.0);
        let sim = jaro_winkler("Johnathan Doe", "Jonathan Doe");
        assert!(sim > 0.90, "sim was {sim}");
        let sim2 = jaro_winkler("Acme Corp Ltd", "Acme Corporation Limited");
        assert!(sim2 > 0.70, "sim2 was {sim2}");
    }
}
