"""
Knuth-Bendix Rewriting System for {4,n} Hyperbolic Tilings
============================================================

Generalized for any regular {4,n} tiling — squares with n meeting at each
vertex. For n >= 5 these live in the hyperbolic plane.

The orientation-preserving symmetry group is presented as:

    G = < A, a, B | Aa = aA = e, B^4 = e, (AB)^n = e >

where:
    A = step forward through the edge you're facing (infinite order)
    a = A^{-1} = step backward
    B = turn left 90° in your current cell (order 4)

The Knuth-Bendix procedure computes rewrite rules that reduce any word to
its shortlex-minimal canonical form. For hyperbolic groups the completed
system is infinite but regular (automatic group); we truncate to a given
max LHS length.

Usage:
    rw = HyperbolicRewriter(vertex_order=5, max_length=30)
    print(rw.reduce("AABBBA"))

    # Convenience function with caching:
    from hyperbolic_rewrite import canonical
    print(canonical("ABABABABAB", vertex_order=5))
"""

from typing import Optional


# ---------------------------------------------------------------------------
# Word utilities
# ---------------------------------------------------------------------------

def word_lt_shortlex(u: str, v: str) -> bool:
    """
    Shortlex order on words over alphabet {A, a, B}.
    Shorter words are smaller; ties broken lexicographically with A < a < B.
    """
    if len(u) != len(v):
        return len(u) < len(v)
    order = {'A': 0, 'a': 1, 'B': 2}
    for cu, cv in zip(u, v):
        if order[cu] != order[cv]:
            return order[cu] < order[cv]
    return False


def invert_word(w: str) -> str:
    """
    Invert a word: reverse it and swap A <-> a, B -> BBB (since B^{-1} = B^3).
    The result is NOT reduced — pass it through reduce() afterward.
    """
    inv_map = {'A': 'a', 'a': 'A', 'B': 'BBB'}
    return ''.join(inv_map[c] for c in reversed(w))


# ---------------------------------------------------------------------------
# Core rewriting engine
# ---------------------------------------------------------------------------

class HyperbolicRewriter:
    """
    Knuth-Bendix completion (truncated to a max LHS length) and word
    reduction for <A, a, B | Aa=aA=e, B^4=e, (AB)^n=e>.
    """

    def __init__(self, vertex_order: int = 5, max_length: int = 30):
        """
        Parameters
        ----------
        vertex_order : int
            Number of squares meeting at each vertex (n in {4,n}). Must be >= 5.
        max_length : int
            Maximum LHS length for generated rules. Words up to this length
            are guaranteed to reach their shortlex normal form.
        """
        if vertex_order < 5:
            raise ValueError(
                f"vertex_order must be >= 5 for hyperbolic tilings, got {vertex_order}. "
                f"(n=4 is Euclidean — just use Z^2!)"
            )
        self.vertex_order = vertex_order
        self.max_length = max_length
        self.rules: list[tuple[str, str]] = []
        self._build_rules()

    def _build_rules(self):
        """
        Run truncated Knuth-Bendix completion. Seeds the initial rules from
        the presentation, then iteratively resolves critical pairs (overlaps
        between LHS's of existing rules) until no new rules within the
        length bound are generated.
        """
        n = self.vertex_order

        # --- Seed rules from the group presentation ---
        initial = [
            ("Aa", ""),             # free cancellation
            ("aA", ""),             # free cancellation
            ("BBBB", ""),           # B^4 = e
            ("AB" * n, ""),         # (AB)^n = e
            ("BA" * n, ""),         # (BA)^n = e  (conjugate: a(AB)^n A = (BA)^n)
        ]

        rules_set: set[tuple[str, str]] = set()
        for lhs, rhs in initial:
            oriented = self._orient(lhs, rhs)
            if oriented[0] is not None:
                rules_set.add(oriented)

        # --- Completion loop ---
        # Each iteration finds all critical pairs among current rules,
        # reduces both sides, and adds new rules when they differ.
        changed = True
        while changed:
            changed = False
            rules_list = sorted(rules_set, key=lambda r: (len(r[0]), r[0]))
            new_rules: set[tuple[str, str]] = set()

            for i, (lhs1, rhs1) in enumerate(rules_list):
                for j, (lhs2, rhs2) in enumerate(rules_list):
                    # Overlap: suffix of lhs1 = prefix of lhs2, length k
                    # The ambient word is lhs1[:-k] + lhs1[-k:] + lhs2[k:]
                    # which rewrites two ways, giving the critical pair.
                    for k in range(1, min(len(lhs1), len(lhs2)) + 1):
                        if lhs1[-k:] == lhs2[:k]:
                            cp1 = rhs1 + lhs2[k:]
                            cp2 = lhs1[:-k] + rhs2

                            r1 = self._reduce_with_rules(cp1, rules_list)
                            r2 = self._reduce_with_rules(cp2, rules_list)

                            if r1 != r2:
                                oriented = self._orient(r1, r2)
                                if oriented[0] is not None:
                                    new_lhs, new_rhs = oriented
                                    if len(new_lhs) <= self.max_length:
                                        if (new_lhs, new_rhs) not in rules_set:
                                            new_rules.add((new_lhs, new_rhs))

            if new_rules:
                rules_set.update(new_rules)
                changed = True
                rules_set = self._interreduce(rules_set)

        self.rules = sorted(rules_set, key=lambda r: (len(r[0]), r[0]))

    def _orient(self, u: str, v: str) -> tuple[Optional[str], Optional[str]]:
        """Orient a pair as a rewrite rule: shortlex-greater -> smaller."""
        if u == v:
            return (None, None)
        return (v, u) if word_lt_shortlex(u, v) else (u, v)

    def _reduce_with_rules(self, word: str, rules: list[tuple[str, str]]) -> str:
        """
        Apply rules to a word until no more apply. Scans left-to-right,
        applying the first matching rule, then restarting the scan.
        """
        changed = True
        while changed:
            changed = False
            for lhs, rhs in rules:
                idx = word.find(lhs)
                if idx != -1:
                    word = word[:idx] + rhs + word[idx + len(lhs):]
                    changed = True
                    break
        return word

    def _interreduce(self, rules_set: set[tuple[str, str]]) -> set[tuple[str, str]]:
        """
        Clean up: reduce each rule's RHS using all other rules, and discard
        any rule whose LHS is already reducible by a shorter rule to the
        same result (i.e., it's subsumed).
        """
        rules_list = sorted(rules_set, key=lambda r: (len(r[0]), r[0]))
        cleaned: set[tuple[str, str]] = set()

        for i, (lhs, rhs) in enumerate(rules_list):
            others = [r for j, r in enumerate(rules_list) if j != i]
            new_rhs = self._reduce_with_rules(rhs, others)

            shorter = [(l, r) for l, r in others if len(l) < len(lhs)]
            lhs_reduced = self._reduce_with_rules(lhs, shorter)
            if lhs_reduced != lhs:
                lhs_fully = self._reduce_with_rules(lhs, others)
                if lhs_fully == new_rhs:
                    continue  # redundant

            if new_rhs == lhs:
                continue
            oriented = self._orient(lhs, new_rhs)
            if oriented[0] is not None:
                cleaned.add(oriented)

        return cleaned

    def reduce(self, word: str) -> str:
        """
        Reduce a word to its shortlex canonical form.

        Raises ValueError if the word exceeds max_length or contains
        invalid characters.
        """
        if len(word) > self.max_length:
            raise ValueError(
                f"Word length {len(word)} exceeds max_length {self.max_length}. "
                f"Rebuild with a larger max_length."
            )
        if not all(c in 'AaB' for c in word):
            raise ValueError(f"Word must only contain A, a, B. Got: {word!r}")
        return self._reduce_with_rules(word, self.rules)

    def __repr__(self):
        return (f"HyperbolicRewriter(vertex_order={self.vertex_order}, "
                f"max_length={self.max_length}, rules={len(self.rules)})")


# ---------------------------------------------------------------------------
# Convenience wrapper with memoized construction
# ---------------------------------------------------------------------------

_cache: dict[tuple[int, int], HyperbolicRewriter] = {}


def get_rewriter(vertex_order: int = 5, max_length: int = 30) -> HyperbolicRewriter:
    """Get or build a cached rewriter for the given parameters."""
    for (vo, ml), rw in _cache.items():
        if vo == vertex_order and ml >= max_length:
            return rw
    rw = HyperbolicRewriter(vertex_order, max_length)
    _cache[(vertex_order, max_length)] = rw
    return rw


def canonical(word: str, vertex_order: int = 5,
              max_length: Optional[int] = None) -> str:
    """
    Top-level convenience: reduce a word to canonical form in {4,n}.
    """
    if max_length is None:
        max_length = max(30, len(word))
    return get_rewriter(vertex_order, max_length).reduce(word)


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def run_tests():
    import random
    from itertools import product as iproduct

    all_ok = True

    for n in [5, 6, 7]:
        print(f"\n{'='*64}")
        print(f"  {{4, {n}}} tiling  —  B^4 = (AB)^{n} = e")
        print(f"{'='*64}")

        rw = HyperbolicRewriter(vertex_order=n, max_length=80)
        print(f"  {len(rw.rules)} rewrite rules generated\n")
        for lhs, rhs in rw.rules:
            print(f"    {lhs:>40s}  ->  {rhs if rhs else 'e'}")
        print()

        # --- Check defining relations ---
        failures = 0

        def check(desc, word, expected):
            nonlocal failures
            result = rw.reduce(word)
            ok = result == expected
            tag = "pass" if ok else "FAIL"
            if not ok:
                print(f"  [{tag}] {desc}: {word} -> {result}, expected {expected or 'e'}")
                failures += 1
            else:
                print(f"  [{tag}] {desc}")

        print("  Defining relations:")
        check("Aa = e", "Aa", "")
        check("aA = e", "aA", "")
        check("B^4 = e", "BBBB", "")
        check(f"(AB)^{n} = e", "AB" * n, "")
        check(f"(BA)^{n} = e", "BA" * n, "")
        check(f"(AB)^{2*n} = e", "AB" * (2 * n), "")
        print()

        # --- Randomized checks ---
        print("  Randomized checks (500 trials x 3 properties)...")
        random.seed(42 + n)
        for trial in range(500):
            length = random.randint(1, 18)
            word = ''.join(random.choice('AaB') for _ in range(length))
            reduced = rw.reduce(word)

            # Idempotency: reduce(reduce(w)) == reduce(w)
            if rw.reduce(reduced) != reduced:
                print(f"    FAIL idempotency: {word} -> {reduced} -> {rw.reduce(reduced)}")
                failures += 1

            # Inverse: w * w^{-1} = e
            inv = invert_word(word)
            try:
                prod = rw.reduce(word + inv)
                if prod != '':
                    print(f"    FAIL inverse: {word} * inv -> {prod}")
                    failures += 1
            except ValueError:
                pass  # concatenation too long, skip

            # Identity insertion: inserting a relator doesn't change the result
            pos = random.randint(0, len(word))
            relator = random.choice(['BBBB', 'Aa', 'aA', 'AB' * n, 'BA' * n])
            padded = word[:pos] + relator + word[pos:]
            try:
                if rw.reduce(padded) != reduced:
                    print(f"    FAIL identity: {word}->{reduced}, padded->{rw.reduce(padded)}")
                    failures += 1
            except ValueError:
                pass

        if failures == 0:
            print("    All passed!")
        else:
            print(f"    {failures} FAILURES")
            all_ok = False
        print()

        # --- Growth rate (element counting) ---
        print("  Distinct group elements reachable by word length:")
        prev = 0
        for L in range(8):
            seen = set()
            for letters in iproduct('AaB', repeat=L):
                seen.add(rw.reduce(''.join(letters)))
            count = len(seen)
            ratio = f"  (x{count/prev:.3f})" if prev > 0 else ""
            print(f"    length {L}: {3**L:>5d} words -> {count:>5d} elements{ratio}")
            prev = count
        print()

    if all_ok:
        print("All tests passed across all orders!")
    else:
        print("SOME TESTS FAILED")

    return all_ok


if __name__ == "__main__":
    run_tests()
