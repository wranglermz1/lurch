---
paths:
  - "docs/**/*.md"
  - "README.md"
---

# Docs

- `docs/` records behaviour recovered from the oracle: conditions, order, constants, the
  names of settings, ConVars and datamap fields, and RVAs. Never decompiler output.
- Give addresses as RVAs into `dumps/current.bin`. Name the build when it is not
  5dba2a2f-02ae8471.
- Say plainly what is unverified, unidentified or not ported; never round it up.
- Use tables for offsets, settings and call trees. Keep each fact in one doc and point to
  it from the others.
- Recorded values (from a recording's settings blob) and image values (ConVars read from
  the dump) say which they are.
- `docs/superpowers/specs/` holds dated design records. Leave them as written.
