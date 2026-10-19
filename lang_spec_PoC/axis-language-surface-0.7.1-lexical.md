# **Axis Surface Language — Lexical Specification**

## **Version 0.7 (Surface)**

**Status:** Normative
**Applies to:** `axis-language-surface-0.7.md`

---

## **1. Purpose**

This document defines the **lexical structure** of the Axis Surface Language
(version 0.7).

It specifies how raw source text is converted into a stream of tokens
prior to parsing.

This document is the **single source of truth for lexing**.
If a behavior is not defined here, the lexer **MUST reject it**.

---

## **2. Scope and Non-Goals**

### In Scope

* Character set
* Whitespace rules
* Comments
* Keywords
* Identifiers
* Literals
* Punctuation and symbols
* Error handling
* Span semantics

### Out of Scope

* Grammar / parsing rules
* Name resolution
* Type checking
* Registry interaction
* Semantics or execution
* Any notion of hierarchy or namespacing

---

## **3. Character Set**

Axis Surface source code is defined over:

* **ASCII characters only** (`U+0000` – `U+007F`)

Any non-ASCII character **MUST cause a lexical error**.

---

## **4. Whitespace**

### **4.1 Definition**

Whitespace characters are:

* Space (`' '`)
* Horizontal tab (`'\t'`)
* Newline (`'\n'`)
* Carriage return (`'\r'`)

### **4.2 Semantics**

* Whitespace is **non-semantic**
* Whitespace separates tokens
* Whitespace may appear anywhere between tokens
* Whitespace MUST NOT appear inside tokens (except string literals)

---

## **5. Comments**

### **5.1 Line Comments**

Axis Surface supports **line comments**.

```axis
// this is a comment
```

Rules:

* A line comment begins with `//`
* It continues until the end of the line
* Comments are treated as whitespace
* Comments do not produce tokens

### **5.2 Block Comments**

Block comments are **not supported** in Surface-0.7.

---

## **6. Tokens**

The lexer produces a linear stream of tokens.

Every token has:

* a **kind**
* a **lexeme** (when applicable)
* a **span** (byte offsets in source)

---

## **7. Keywords (Reserved Words)**

The following identifiers are **reserved keywords** and MUST be emitted as
distinct token kinds:

```text
fn
let
if
else
match
enum
proj
```

Rules:

* Keywords are **case-sensitive**
* Keywords MUST NOT be emitted as identifiers
* Identifiers containing keyword substrings are valid identifiers

Examples:

* `fn` → keyword
* `proj` → keyword
* `fnx` → identifier
* `project` → identifier

---

## **8. Identifiers**

### **8.1 Definition (Normative)**

An identifier is defined as:

```
identifier ::= letter { letter | digit | "_" }
```

Where:

* `letter` ::= `A`–`Z` | `a`–`z`
* `digit` ::= `0`–`9`

### **8.2 Constraints**

* Identifiers are **case-sensitive**
* Identifiers are **flat and atomic**
* Identifiers **MUST NOT contain `.` (dot)**
* Identifiers **MUST NOT encode hierarchy**

### **8.3 Semantics**

* Identifiers carry **no semantic meaning at lexing time**
* There are **no qualified identifiers**
* There is **no namespacing syntax**
* All structure and hierarchy are declared **outside surface syntax**
  (via registries, contracts, or views)

Any identifier containing a `.` character **MUST cause a lexical error**.

---

## **9. Literals**

### **9.1 Integer Literals**

```
int_literal ::= digit { digit }
```

Rules:

* Integers are base-10 only
* No sign is included in the literal
* Negative numbers are represented using operators
* Leading zeros are permitted

Invalid examples:

* `12a`
* `1_000` (underscores not permitted)

---

### **9.2 Boolean Literals**

Boolean literals are:

```text
true
false
```

Rules:

* Emitted as distinct token kinds `TRUE` and `FALSE`

---

### **9.3 Unit Literal**

The unit literal is the exact two-character sequence:

```
()
```

Rules:

* Emitted as a single `UNIT` token
* No whitespace permitted between `(` and `)`
* Any other parenthesized form lexes as `LPAREN … RPAREN`

---

### **9.4 String Literals**

```
string_literal ::= '"' { character | escape } '"'
```

Supported escape sequences:

```
\"   \\   \n   \t
```

Rules:

* Strings MUST be terminated
* Invalid escape sequences are lexical errors
* Newlines inside strings are not permitted unless escaped

Invalid examples:

* `"unterminated`
* `"\q"`

---

## **10. Punctuation and Symbols**

The following punctuation tokens are recognized:

| Symbol | Token      |
| -----: | ---------- |
|    `{` | LBRACE     |
|    `}` | RBRACE     |
|    `(` | LPAREN     |
|    `)` | RPAREN     |
|    `,` | COMMA      |
|    `:` | COLON      |
|   `=>` | FAT_ARROW  |
|   `->` | THIN_ARROW |

Rules:

* Multi-character tokens (e.g. `=>`) MUST be recognized greedily
* No other punctuation is admitted

Notably:

* There is **no DOT token**
* There is **no field-access operator**
* There is **no tuple indexing punctuation**
* There is **no module or namespace punctuation**

All projection and access semantics are expressed via **keywords**, not symbols.

---

## **11. Token Spans**

### **11.1 Definition**

Every token MUST carry a span defined as:

```
[start_byte, end_byte)
```

Where:

* `start_byte` is inclusive
* `end_byte` is exclusive

Spans refer to byte offsets in the source file.

---

## **12. Error Handling**

### **12.1 Illegal Characters**

Any character not admitted by this specification **MUST cause a lexical error**.

Example:

```text
@
```

→ error at position 0.

---

### **12.2 Invalid Token Forms**

The following MUST cause lexical errors:

* Unterminated string literals
* Invalid escape sequences
* Malformed numeric literals
* Non-ASCII characters
* Identifiers containing `.`
* Any punctuation not listed in §10

Lexical errors MUST:

* halt lexing immediately
* report an exact span
* provide a clear error category

---

## **13. Determinism Guarantee**

Given identical source input, the lexer MUST:

* produce identical token streams
* produce identical spans
* fail at the same position with the same error

---

## **14. Relationship to Parsing**

The lexer:

* does not know grammar
* does not know semantics
* does not consult the registry
* does not recover from errors

Its sole responsibility is **faithful tokenization** under this specification.

---

## **15. Versioning**

This lexical specification is bound to:

```
Axis Surface Language v0.7
```

Any change to lexing rules requires:

* a version bump
* updated CP-1 tests
* explicit documentation

---

## **Guiding Principle**

> **If the lexer accepts it, the language allowed it.
> If the language did not allow it, the lexer MUST reject it.**

---

### **End of Document**