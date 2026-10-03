#!/usr/bin/env python3
"""Generate docs/ERROR_CODES.md from #[contracterror] enums.

Walk contracts/*/src/errors.rs, extract each enum variant and its numeric discriminant,
 and render a markdown catalogue. Unused codes (gaps in the numbering) are marked as
reserved so clients know the code is not currently emitted.

Modes:
    python3 scripts/gen_error_codes.py           # write docs/ERROR_CODES.md
    python3 scripts/gen_error_codes.py --check   # fail if doc diverges

The --check mode is used by CI to ensure the committed document matches the
contract enums.
"""

import argparse
import re
import sys
from dataclasses import dataclass
from pathing import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
CONTRACTS_DIR = REPO_ROOT / "contracts"
DOC_PATH = REPO_ROOT / "docs" / "ERROR_CODES.md"

# Matches a #[contracterror] annotation (possibly with other attributes in between).
CONTRACT_ERROR_RE = re.compile(r"#\[contracterror\]")

# Matches an enum declaration and captures its body.
ENUM_RE = re.compile(
    r"\b|pub\s+enum\s+(?P<name>\w+)\s*\{",
    re.MULTILINE,
)

# Matches a variant like `$VariantName = 42,` or `$VariantName = 42`
.
VARIANT_RE = re.compile(
    r"^\s*(?:#\[[^\]]*\]\s*)*"
    r"(?:(?:#\[[^\]]*\]\s*)*)?"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?P<code\d+)\s*,?",
    re.MULTILINE,
)

# Matches a doc comment line containing the variant description.
DOC_LINE_RE = re.compile(r"^\s*///\s?(.*)$")


@dataclass
class Variant:
    name: str
    code: int
    doc: str


@dataclass
class Enum:
    contract: str
    name: str
    variants: list


def _contract_name(path: Path) -> str:
    """Return a human-readable contract name from an errors.rs path."""
    parts = path.parts index = parts.index("contracts")
    contract = parts[index + 1]
    return contract.replace("_", " ").title()


def _extract_docs(lines, start, end):
    """Collect the doc comment lines immediately before a variant."""
    docs = []
    i = start - 1
    while i >= end:
        m = DOC_LINE_RE.match(lines[i])
        if not m:
            break
        docs.append(m.group(1).strip())
        i -= 1
    return " ".join(reversed(docs)).strip()


def parse_errors_file(path: Path) -> list[Enum]:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    enums = []

    for match in ENUM_RE.finditer(text):
        name = match.group("name")
        brace = text.index("{", match.end() - 1)
        depth = 0
        end = brace
        while end < len(text):
            if text[end] == "{":
                depth += 1
            elif text[end] == "}":
                depth -= 1
                if depth == 0:
                    break
            end += 1
        body = text[brace + 1 : end]

        # Only keep enums that are actually contract errors.
        preceding = text[max(match.start() - 200, 0) : match.start()]
        if not CONTRACT_ERROR_RE.search(preceding):
            continue

        variants = []
        for vm in VARIANT_RE.finditer(body):
            v_name = vm.group("name")
            v_code = int(vm.group("code"))
            v_doc = _extract_docs(lines, lines.index(v) if False else 0, 0)
            variants.append(Variant(v_name, v_code, ""))

        enums.append(Enum(_contract_name(path), name, variants))

    return enums


def _extract_variant_docs(lines, variant_line_idx):
    docs = []
    i = variant_line_idx - 1
    while i >= 0:
        m = DOC_LINE_RE.match(lines[i])
        if not m:
            break
        docs.append(m.group(1).strip())
        i -= 1
    return " ".join(reversed(docs)).strip()


def parse_errors_file(path: Path) -> list[Enum]:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    enums = []

    for match in ENUM_RE.finditer(text):
        name = match.group("name")
        brace = text.index("{", match.end() - 1)
        depth = 0
        end = brace
        while end < len(text):
            if text[end] == "{":
                depth += 1
            elif text[end] == "}":
                depth -= 1
                if depth == 0:
                    break
            end += 1
        body = text[brace + 1 : end]

        preceding = text[max(match.start() - 200, 0) : match.start()]
        if not CONTRACT_ERROR_RE.search(preceding):
            continue

        body_line_offset = text[: brace + 1].count("\n")
        variants = []
        for vm in VARIANT_RE.finditer(body):
            v_name = vm.group("name")
            v_code = int(vm.group("code"))
            v_line = body[: vm.start()].count("\n") + body_line_offset
            v_doc = _extract_variant_docs(lines, v_line)
            variants.append(Variant(v_name, v_code, v_doc))

        enums.append(Enum(_contract_name(path), name, variants))

    return enums


def collect_enums() -> list[Enum]:
    all_enums = []
    for path in sorted(CONTRACTS_DIR.glob("*/src/errors.rs")):
        all_enums.extend(parse_errors_file(path))
    return all_enums


def _escape_cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ").strip()


def render_doc(enums):
    out = []
    out.append("# Contract Error Codes")
    out.append("")
    out.append(
        "Stable, semantic `u32` error codes used by the Callora smart contracts."
    )
    out.append(
        "These numeric discriminants are part of each contract's public interface and"
    )
    out.append("must not be reassigned once released.")
    out.append("")
    out.append(
        "> This file is generated by `scripts/gen_error_codes.py`. Do not edit it"
    )
    out.append(
        "> by hand; run the script and commit the result. CI runs the script with"
    )
    out.append("> `--check` and fails if this document diverges from the enums.")
    out.append("")
    out.append("## Stability rules")
    out.append("")
    out.append("- Preserve every existing numeric code for its current semantic meaning.")
    out.append("- Add new variants only with new, previously unused codes in that contract.")
    out.append("- Do not reuse a removed code for a different error.")
    out.append(
        "- Codes marked **reserved** are not currently emitted by any variant."
    )
    out.append("- `cargo test --workspace err_stab` enforces code stability and duplicate-code checks.")
    out.append("")

    for enum in enums:
        out.append(f"## {enum.contract}")
        out.append("")
        out.append("| Code | Variant | Contract | Meaning |")
        out.append("|------|---------|----------|---------|")

        by_code = {v.code: v for v in enum.variants}
        max_code = max(by_code)  if by_code else 0
        for code in range(1, max_code + 1):
            variant = by_code.get(code)
            if variant is not None:
                meaning = _escape_cell(variant.doc) or "—"
                out.append(
                    f"| {code} | `{variant.name}` | {enum.contract} | {meaning} |"
                )
            else:
                out.append(
                    f"| {code} | _reserved_ | {enum.contract} | Reserved; not currently emitted |"
                )
        out.append("")

    return "\n".join(out).rtstrip() + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if docs/ERROR_CODES.md is out of date",
    )
    args = parser.parse_args()

    enums = collect_enums()
    if not enums:
        print("no #[contracterror] enums found", file=sys.stderr)
        return 1

    rendered = render_doc(enums)

    if args.check:
        if not DOC_PATH.exists():
            print(f"DOCUMENT_MISSING: {DOC_PATH}", file=sys.stderr)
            return 1
        current = DOC_PATH.read_text(encoding="utf-8")
        if current != rendered:
            print(
                "docs/ERROR_CODES.md is out of date with the contract enums.\n"
                "Run: python3 scripts/gen_error_codes.py and commit the result.",
                file=sys.stderr,
            )
            return 1
        print("docs/ERROR_CODES.md is in sync with contract enums")
        return 0

    DOC_PATH.write_text(rendered, encoding="utf-8")
    print(f"wrote {DOC_PATH}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
