# Close the rust - js (boa) conformity gap

### Preface:
The boa team has released v0.22 (28AGU2026), acheiving a conformity of 95.60% in the official ECMAScript Test Suite (Test262), up from 89.92% in our current version v0.20 (05DEC2024). At only  about 90% conformance from the current engine, we are certainly missing modern language features, syntax constructs and scoping behaviors vanilla js will support. Furthermore, at nearly, 2 years old, the older version is certainly out of support.

Whilst this project does have a defined custom_rules syntax which the current boa engine v0.20 parses, the nature of this implementation is that we are running untested, untrusted code from the oxvelte app perspective, thus ensuring maximal conformity of ECMAScript-262 is vital to ensure users can pass custom rules written in js using a syntax they expect. Likewise, under our own development, we want to ensure we parse the known standard accurately, and do no rely on older broken quirks in an expirimental runtime that are likely already out of support.

Upgrading boa 0.20 to 0.22 is a breaking change, thus this document servers as primary the research document to notate and plan for the adapting our boa wrapper to support it.