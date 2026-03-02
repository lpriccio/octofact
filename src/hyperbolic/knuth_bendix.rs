//! Knuth-Bendix rewriting system for {4,n} hyperbolic tilings.
//!
//! The orientation-preserving symmetry group is:
//!
//! ```text
//! G = < A, a, B | Aa = aA = e, B^4 = e, (AB)^n = e >
//! ```
//!
//! where:
//! - **A** = step forward (cross the edge you're facing) — infinite order
//! - **a** = A⁻¹ = step backward
//! - **B** = turn left 90° — order 4
//!
//! The Knuth-Bendix procedure computes rewrite rules that reduce any word to
//! its shortlex-minimal canonical form. For hyperbolic groups the completed
//! system is infinite but regular (automatic group); we truncate to a given
//! max LHS length.

/// Letter constants.
pub const FWD: u8 = b'A';
pub const BACK: u8 = b'a';
pub const LEFT: u8 = b'B';

/// A word in the free monoid over {A, a, B}.
pub type Word = Vec<u8>;

/// Shortlex order: shorter words are smaller; ties broken lexicographically
/// with A < a < B.
fn letter_rank(c: u8) -> u8 {
    match c {
        FWD => 0,
        BACK => 1,
        LEFT => 2,
        _ => 255,
    }
}

/// Returns true if `u` is strictly less than `v` in shortlex order.
pub fn word_lt_shortlex(u: &[u8], v: &[u8]) -> bool {
    if u.len() != v.len() {
        return u.len() < v.len();
    }
    for (&cu, &cv) in u.iter().zip(v.iter()) {
        let ru = letter_rank(cu);
        let rv = letter_rank(cv);
        if ru != rv {
            return ru < rv;
        }
    }
    false
}

/// Invert a word: reverse and swap A↔a, B→BBB (since B⁻¹ = B³).
/// The result is NOT reduced.
pub fn invert_word(w: &[u8]) -> Word {
    let mut result = Vec::with_capacity(w.len() * 3); // worst case: all B's
    for &c in w.iter().rev() {
        match c {
            FWD => result.push(BACK),
            BACK => result.push(FWD),
            LEFT => {
                result.push(LEFT);
                result.push(LEFT);
                result.push(LEFT);
            }
            _ => {}
        }
    }
    result
}

/// A completed (truncated) Knuth-Bendix rewriting system.
pub struct RewriteSystem {
    /// The vertex order n in {4,n}.
    pub vertex_order: u32,
    /// Maximum LHS length for rules.
    pub max_length: usize,
    /// Rewrite rules: (lhs, rhs) where lhs → rhs, sorted by (len(lhs), lhs).
    pub rules: Vec<(Word, Word)>,
}

impl RewriteSystem {
    /// Build a rewriting system for the {4,n} tiling group.
    ///
    /// `vertex_order` must be >= 5 (hyperbolic). `max_length` controls the
    /// maximum LHS length for generated rules.
    pub fn new(vertex_order: u32, max_length: usize) -> Self {
        assert!(
            vertex_order >= 5,
            "vertex_order must be >= 5 for hyperbolic tilings, got {vertex_order}"
        );

        let n = vertex_order as usize;

        // Seed rules from the group presentation.
        let mut initial: Vec<(Word, Word)> = Vec::new();
        initial.push((vec![FWD, BACK], vec![])); // Aa = e
        initial.push((vec![BACK, FWD], vec![])); // aA = e
        initial.push((vec![LEFT, LEFT, LEFT, LEFT], vec![])); // B^4 = e

        // (AB)^n = e
        let mut ab_n = Vec::with_capacity(2 * n);
        for _ in 0..n {
            ab_n.push(FWD);
            ab_n.push(LEFT);
        }
        initial.push((ab_n, vec![]));

        // (BA)^n = e
        let mut ba_n = Vec::with_capacity(2 * n);
        for _ in 0..n {
            ba_n.push(LEFT);
            ba_n.push(FWD);
        }
        initial.push((ba_n, vec![]));

        let mut rules_set: Vec<(Word, Word)> = Vec::new();
        for (lhs, rhs) in initial {
            if let Some(oriented) = orient(&lhs, &rhs) {
                if !rules_set.contains(&oriented) {
                    rules_set.push(oriented);
                }
            }
        }

        // Completion loop.
        let mut changed = true;
        while changed {
            changed = false;
            rules_set.sort_by(|a, b| {
                a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0))
            });

            let mut new_rules: Vec<(Word, Word)> = Vec::new();

            for i in 0..rules_set.len() {
                for j in 0..rules_set.len() {
                    let (ref lhs1, ref rhs1) = rules_set[i];
                    let (ref lhs2, ref rhs2) = rules_set[j];

                    let max_k = lhs1.len().min(lhs2.len());
                    for k in 1..=max_k {
                        if lhs1[lhs1.len() - k..] == lhs2[..k] {
                            // Critical pair.
                            let mut cp1 = rhs1.clone();
                            cp1.extend_from_slice(&lhs2[k..]);

                            let mut cp2 = lhs1[..lhs1.len() - k].to_vec();
                            cp2.extend_from_slice(rhs2);

                            let r1 = reduce_with_rules(&cp1, &rules_set);
                            let r2 = reduce_with_rules(&cp2, &rules_set);

                            if r1 != r2 {
                                if let Some(oriented) = orient(&r1, &r2) {
                                    let (ref new_lhs, _) = oriented;
                                    if new_lhs.len() <= max_length
                                        && !rules_set.contains(&oriented)
                                        && !new_rules.contains(&oriented)
                                    {
                                        new_rules.push(oriented);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !new_rules.is_empty() {
                rules_set.extend(new_rules);
                changed = true;
                rules_set = interreduce(rules_set);
            }
        }

        rules_set.sort_by(|a, b| {
            a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0))
        });

        RewriteSystem {
            vertex_order,
            max_length,
            rules: rules_set,
        }
    }

    /// Reduce a word to its shortlex canonical form.
    pub fn reduce(&self, word: &[u8]) -> Word {
        reduce_with_rules(word, &self.rules)
    }
}

/// Orient a pair as a rewrite rule: shortlex-greater → smaller.
/// Returns `None` if u == v.
fn orient(u: &[u8], v: &[u8]) -> Option<(Word, Word)> {
    if u == v {
        return None;
    }
    if word_lt_shortlex(u, v) {
        Some((v.to_vec(), u.to_vec()))
    } else {
        Some((u.to_vec(), v.to_vec()))
    }
}

/// Apply rules left-to-right until no more apply.
fn reduce_with_rules(word: &[u8], rules: &[(Word, Word)]) -> Word {
    let mut w: Word = word.to_vec();
    let mut changed = true;
    while changed {
        changed = false;
        for (lhs, rhs) in rules {
            if let Some(idx) = find_subslice(&w, lhs) {
                let mut new_w = Vec::with_capacity(w.len() - lhs.len() + rhs.len());
                new_w.extend_from_slice(&w[..idx]);
                new_w.extend_from_slice(rhs);
                new_w.extend_from_slice(&w[idx + lhs.len()..]);
                w = new_w;
                changed = true;
                break;
            }
        }
    }
    w
}

/// Find the first occurrence of `needle` in `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Interreduce: reduce each rule's RHS using all other rules, discard
/// redundant rules.
fn interreduce(rules_set: Vec<(Word, Word)>) -> Vec<(Word, Word)> {
    let mut rules_list = rules_set;
    rules_list.sort_by(|a, b| {
        a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0))
    });

    let mut cleaned: Vec<(Word, Word)> = Vec::new();

    for i in 0..rules_list.len() {
        let (ref lhs, ref rhs) = rules_list[i];

        // Build "others" list — all rules except this one.
        let others: Vec<(Word, Word)> = rules_list
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, r)| r.clone())
            .collect();

        let new_rhs = reduce_with_rules(rhs, &others);

        // Check if LHS is reducible by shorter rules.
        let shorter: Vec<(Word, Word)> = others
            .iter()
            .filter(|(l, _)| l.len() < lhs.len())
            .cloned()
            .collect();
        let lhs_reduced = reduce_with_rules(lhs, &shorter);

        if lhs_reduced != *lhs {
            let lhs_fully = reduce_with_rules(lhs, &others);
            if lhs_fully == new_rhs {
                continue; // redundant
            }
        }

        if new_rhs == *lhs {
            continue;
        }

        if let Some(oriented) = orient(lhs, &new_rhs) {
            if !cleaned.contains(&oriented) {
                cleaned.push(oriented);
            }
        }
    }

    cleaned
}

/// Helper: convert a Word to a debug string (for display/testing).
pub fn word_to_string(w: &[u8]) -> String {
    if w.is_empty() {
        "e".to_string()
    } else {
        String::from_utf8_lossy(w).to_string()
    }
}

/// Helper: convert a string to a Word.
pub fn string_to_word(s: &str) -> Word {
    s.bytes().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn w(s: &str) -> Word {
        string_to_word(s)
    }

    fn ws(word: &[u8]) -> String {
        word_to_string(word)
    }

    /// Simple LCG-based pseudo-random number generator (deterministic, no deps).
    struct SimpleRng {
        state: u64,
    }

    impl SimpleRng {
        fn new(seed: u64) -> Self {
            SimpleRng { state: seed }
        }
        fn next_u64(&mut self) -> u64 {
            self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.state
        }
        fn next_usize(&mut self, bound: usize) -> usize {
            (self.next_u64() % bound as u64) as usize
        }
        fn random_word(&mut self, max_len: usize) -> Word {
            let letters = [FWD, BACK, LEFT];
            let len = self.next_usize(max_len) + 1;
            (0..len).map(|_| letters[self.next_usize(3)]).collect()
        }
    }

    // -----------------------------------------------------------------------
    // Unit tests for word utilities
    // -----------------------------------------------------------------------

    #[test]
    fn test_shortlex_ordering() {
        // Shorter is smaller.
        assert!(word_lt_shortlex(&w("A"), &w("AA")));
        assert!(word_lt_shortlex(&w(""), &w("A")));
        // Same length: lexicographic with A < a < B.
        assert!(word_lt_shortlex(&w("A"), &w("a")));
        assert!(word_lt_shortlex(&w("A"), &w("B")));
        assert!(word_lt_shortlex(&w("a"), &w("B")));
        // Equal words: not less.
        assert!(!word_lt_shortlex(&w("AB"), &w("AB")));
        // Multi-letter.
        assert!(word_lt_shortlex(&w("AA"), &w("AB")));
        assert!(word_lt_shortlex(&w("Aa"), &w("AB")));
    }

    #[test]
    fn test_invert_word() {
        assert_eq!(invert_word(&w("A")), w("a"));
        assert_eq!(invert_word(&w("a")), w("A"));
        assert_eq!(invert_word(&w("B")), w("BBB"));
        assert_eq!(invert_word(&w("AB")), w("BBBa"));
        assert_eq!(invert_word(&w("")), w(""));
    }

    // -----------------------------------------------------------------------
    // Defining relations
    // -----------------------------------------------------------------------

    fn check_defining_relations(rw: &RewriteSystem) {
        let n = rw.vertex_order as usize;

        assert_eq!(rw.reduce(&w("Aa")), w(""), "Aa should reduce to e");
        assert_eq!(rw.reduce(&w("aA")), w(""), "aA should reduce to e");
        assert_eq!(rw.reduce(&w("BBBB")), w(""), "B^4 should reduce to e");

        // (AB)^n = e
        let ab_n: Word = "AB".repeat(n).bytes().collect();
        assert_eq!(rw.reduce(&ab_n), w(""), "(AB)^n should reduce to e");

        // (BA)^n = e
        let ba_n: Word = "BA".repeat(n).bytes().collect();
        assert_eq!(rw.reduce(&ba_n), w(""), "(BA)^n should reduce to e");

        // (AB)^{2n} = e
        let ab_2n: Word = "AB".repeat(2 * n).bytes().collect();
        assert_eq!(rw.reduce(&ab_2n), w(""), "(AB)^{{2n}} should reduce to e");
    }

    #[test]
    fn test_defining_relations_n5() {
        let rw = RewriteSystem::new(5, 80);
        check_defining_relations(&rw);
    }

    #[test]
    fn test_defining_relations_n6() {
        let rw = RewriteSystem::new(6, 80);
        check_defining_relations(&rw);
    }

    #[test]
    fn test_defining_relations_n7() {
        let rw = RewriteSystem::new(7, 80);
        check_defining_relations(&rw);
    }

    // -----------------------------------------------------------------------
    // Rule counts (match Python reference)
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_counts() {
        let rw5 = RewriteSystem::new(5, 80);
        assert_eq!(rw5.rules.len(), 8, "n=5 should have 8 rules");

        let rw6 = RewriteSystem::new(6, 80);
        assert_eq!(rw6.rules.len(), 5, "n=6 should have 5 rules");

        let rw7 = RewriteSystem::new(7, 80);
        assert_eq!(rw7.rules.len(), 8, "n=7 should have 8 rules");
    }

    // -----------------------------------------------------------------------
    // Idempotency
    // -----------------------------------------------------------------------

    fn check_idempotency(rw: &RewriteSystem, seed: u64) {
        let mut rng = SimpleRng::new(seed);
        for _ in 0..1000 {
            let word = rng.random_word(18);
            let reduced = rw.reduce(&word);
            let reduced2 = rw.reduce(&reduced);
            assert_eq!(
                reduced, reduced2,
                "idempotency failed: {} -> {} -> {}",
                ws(&word), ws(&reduced), ws(&reduced2)
            );
        }
    }

    #[test]
    fn test_idempotency_n5() {
        let rw = RewriteSystem::new(5, 80);
        check_idempotency(&rw, 42 + 5);
    }

    #[test]
    fn test_idempotency_n6() {
        let rw = RewriteSystem::new(6, 80);
        check_idempotency(&rw, 42 + 6);
    }

    #[test]
    fn test_idempotency_n7() {
        let rw = RewriteSystem::new(7, 80);
        check_idempotency(&rw, 42 + 7);
    }

    // -----------------------------------------------------------------------
    // Inverse cancellation
    // -----------------------------------------------------------------------

    fn check_inverse(rw: &RewriteSystem, seed: u64) {
        let mut rng = SimpleRng::new(seed);
        for _ in 0..1000 {
            let word = rng.random_word(12); // shorter to avoid exceeding max_length
            let inv = invert_word(&word);
            let mut product = word.clone();
            product.extend_from_slice(&inv);
            if product.len() <= rw.max_length {
                let result = rw.reduce(&product);
                assert_eq!(
                    result,
                    w(""),
                    "inverse failed: {} * {} -> {}",
                    ws(&word), ws(&inv), ws(&result)
                );
            }
        }
    }

    #[test]
    fn test_inverse_n5() {
        let rw = RewriteSystem::new(5, 80);
        check_inverse(&rw, 42 + 5);
    }

    #[test]
    fn test_inverse_n6() {
        let rw = RewriteSystem::new(6, 80);
        check_inverse(&rw, 42 + 6);
    }

    #[test]
    fn test_inverse_n7() {
        let rw = RewriteSystem::new(7, 80);
        check_inverse(&rw, 42 + 7);
    }

    // -----------------------------------------------------------------------
    // Identity insertion
    // -----------------------------------------------------------------------

    fn check_identity_insertion(rw: &RewriteSystem, seed: u64) {
        let n = rw.vertex_order as usize;
        let mut rng = SimpleRng::new(seed);
        let relators: Vec<Word> = vec![
            w("BBBB"),
            w("Aa"),
            w("aA"),
            "AB".repeat(n).bytes().collect(),
            "BA".repeat(n).bytes().collect(),
        ];

        for _ in 0..1000 {
            let word = rng.random_word(12);
            let reduced = rw.reduce(&word);

            let pos = rng.next_usize(word.len() + 1);
            let relator = &relators[rng.next_usize(relators.len())];

            let mut padded = word[..pos].to_vec();
            padded.extend_from_slice(relator);
            padded.extend_from_slice(&word[pos..]);

            if padded.len() <= rw.max_length {
                let padded_reduced = rw.reduce(&padded);
                assert_eq!(
                    padded_reduced, reduced,
                    "identity insertion failed: {} -> {}, padded({}, pos={}, rel={}) -> {}",
                    ws(&word), ws(&reduced), ws(&padded), pos, ws(relator), ws(&padded_reduced)
                );
            }
        }
    }

    #[test]
    fn test_identity_insertion_n5() {
        let rw = RewriteSystem::new(5, 80);
        check_identity_insertion(&rw, 42 + 5);
    }

    #[test]
    fn test_identity_insertion_n6() {
        let rw = RewriteSystem::new(6, 80);
        check_identity_insertion(&rw, 42 + 6);
    }

    #[test]
    fn test_identity_insertion_n7() {
        let rw = RewriteSystem::new(7, 80);
        check_identity_insertion(&rw, 42 + 7);
    }

    // -----------------------------------------------------------------------
    // Growth rate (element counting)
    // -----------------------------------------------------------------------

    /// Expected distinct element counts by word length, from Python reference.
    /// Format: [length_0, length_1, ..., length_7]
    fn expected_growth(n: u32) -> Vec<usize> {
        match n {
            5 => vec![1, 3, 8, 20, 48, 114, 270, 636],
            6 => vec![1, 3, 8, 20, 48, 114, 270, 638],
            7 => vec![1, 3, 8, 20, 48, 114, 270, 638],
            _ => panic!("no reference data for n={n}"),
        }
    }

    fn count_distinct_elements(rw: &RewriteSystem, max_word_len: usize) -> Vec<usize> {
        use std::collections::HashSet;
        let letters = [FWD, BACK, LEFT];
        let mut counts = Vec::new();

        for len in 0..=max_word_len {
            let mut seen: HashSet<Word> = HashSet::new();
            if len == 0 {
                seen.insert(rw.reduce(&[]));
            } else {
                // Generate all words of this length.
                let total = 3_usize.pow(len as u32);
                for i in 0..total {
                    let mut word = Vec::with_capacity(len);
                    let mut idx = i;
                    for _ in 0..len {
                        word.push(letters[idx % 3]);
                        idx /= 3;
                    }
                    seen.insert(rw.reduce(&word));
                }
            }
            counts.push(seen.len());
        }
        counts
    }

    #[test]
    fn test_growth_rate_n5() {
        let rw = RewriteSystem::new(5, 80);
        let counts = count_distinct_elements(&rw, 7);
        assert_eq!(counts, expected_growth(5), "growth mismatch for n=5");
    }

    #[test]
    fn test_growth_rate_n6() {
        let rw = RewriteSystem::new(6, 80);
        let counts = count_distinct_elements(&rw, 7);
        assert_eq!(counts, expected_growth(6), "growth mismatch for n=6");
    }

    #[test]
    fn test_growth_rate_n7() {
        let rw = RewriteSystem::new(7, 80);
        let counts = count_distinct_elements(&rw, 7);
        assert_eq!(counts, expected_growth(7), "growth mismatch for n=7");
    }

    // -----------------------------------------------------------------------
    // Benchmark-ish: reduction of random words
    // -----------------------------------------------------------------------

    #[test]
    fn test_reduction_bulk() {
        let rw = RewriteSystem::new(5, 40);
        let mut rng = SimpleRng::new(12345);
        for _ in 0..10_000 {
            let word = rng.random_word(20);
            let reduced = rw.reduce(&word);
            // Just verify it's idempotent.
            assert_eq!(rw.reduce(&reduced), reduced);
        }
    }

    // -----------------------------------------------------------------------
    // Print rules (not a test, but useful for debugging)
    // -----------------------------------------------------------------------

    #[test]
    fn test_print_rules() {
        for n in [5, 6, 7] {
            let rw = RewriteSystem::new(n, 80);
            println!("\n{{4, {n}}} — {} rules:", rw.rules.len());
            for (lhs, rhs) in &rw.rules {
                println!("  {:>40} -> {}", ws(lhs), ws(rhs));
            }
        }
    }
}
