# Licensing

catiakit is available under the
[PolyForm Noncommercial License 1.0.0](../LICENSES/PolyForm-Noncommercial-1.0.0.md).
**UnRobotics Inc.** offers separate commercial agreements for use outside those
permissions. The [license notice](../LICENSE) identifies the licensor and the
required notices to preserve when redistributing the software.

| Intended use | Applicable terms |
| --- | --- |
| Noncommercial use, including personal study or experimentation without an anticipated commercial application | PolyForm, subject to its terms |
| Use by the noncommercial organizations specified in the license | PolyForm, subject to its terms |
| Internal commercial applications or business use outside PolyForm permissions | A written Internal commercial agreement |
| Integration and runtime distribution in a commercial product | A written OEM commercial agreement |
| Customer-operated hosted services or APIs | An agreed SaaS addendum |
| Redistribution as a development SDK | An agreed SDK redistribution addendum |
| Commercial feasibility evaluation outside PolyForm permissions | A separately requested evaluation agreement |

Company size alone does not create an exemption. Cloning the repository or
installing a package does not create a commercial trial or agreement. Consult
the actual PolyForm terms for their full permissions, including the specified
charitable, educational, public research and government organizations.

See [commercial licensing](../COMMERCIAL-LICENSE.md) for available arrangements
and [contact UnRobotics Inc.](https://www.un-robotics.com/#contact) for a
quotation. That information page does not itself grant a license.

## Dependencies and references

Third-party components retain their own terms and notices. See
[third-party notices](../THIRD_PARTY_NOTICES.md) for the Rust dependencies,
the cadmpeg format notes used as a layout reference, and the test data policy.

The Cargo manifests declare `PolyForm-Noncommercial-1.0.0` for project crates.
That expression describes catiakit material only; it is not an alternative
license for the dependencies compiled into a binary.

`tools/license/check_license.py` verifies the notice set, the pinned license
texts and the dependency ledger. Run it with `--write` after changing
`Cargo.lock` to regenerate `LICENSES/rust-dependencies.txt`.

Your input files and the extracted output remain yours; this licensing model
claims no additional ownership or output-based royalties in that data.
