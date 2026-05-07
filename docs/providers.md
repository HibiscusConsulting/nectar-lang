# Providers

Providers are the third layer of Nectar's architecture. Keywords define language primitives, the standard library defines interfaces, and providers fulfill those interfaces with concrete vendor integrations.

## The Three Layers

```
Keywords            payment, banking, db, miniprogram, map, ...
   |
   v
Standard Library    payment::charge, mp::tradePay, banking::transfer, ...
   |                (interfaces declared in compiler/src/stdlib.rs)
   v
Providers           providers/stripe.js, providers/moov.js, providers/alipay.js, ...
                    (concrete HTTP/SDK calls to a vendor)
```

Application code only ever touches the top two layers. Swapping providers is a configuration change, not a code change.

## Built-in Providers

| Provider | File | Domain | Fulfills |
|---|---|---|---|
| Moov | [providers/moov.js](../providers/moov.js) | Banking, ACH, wallets, transfers | `banking`, `payment` |
| Stripe | [providers/stripe.js](../providers/stripe.js) | Card payments, Connect | `payment` |
| Plaid | [providers/plaid.js](../providers/plaid.js) | Bank account linking, transactions | `banking` |
| Alipay | [providers/alipay.js](../providers/alipay.js) | Chinese super-app payments | `miniprogram` (`mp::*`) |
| Mapbox | [providers/mapbox.js](../providers/mapbox.js) | Maps, geocoding, directions | `map`, `maps` |

## Provider Contract

A provider is a JavaScript module that exports the function set declared by a stdlib registration in `compiler/src/stdlib.rs`. The compiler emits WASM imports against the stdlib interface; the provider supplies the implementations at runtime.

For the `payment` keyword, the compiler emits imports such as:

```
payment::charge(amount: BigDecimal, customer_id: String) -> Result<ChargeId, Error>
payment::refund(charge_id: ChargeId, amount: Option<BigDecimal>) -> Result<RefundId, Error>
payment::list_charges(customer_id: String) -> Result<Vec<Charge>, Error>
```

A provider for `payment` (e.g. `stripe.js`) exports JavaScript functions that satisfy those signatures and translate the calls to the Stripe API.

## Switching Providers

Provider selection is configured in `nectar.toml`:

```toml
[providers]
payment = "stripe"      # or "moov"
banking = "moov"        # or "plaid"
miniprogram = "alipay"  # provider for mp:: namespace
map = "mapbox"
```

Application code does not change. The compiler links the WASM imports to the configured provider at build time.

## Adding a New Provider

1. Create `providers/<name>.js` exporting the function set declared by the stdlib module you are fulfilling.
2. Add a `[providers]` entry to `nectar.toml` pointing the keyword to the new provider.
3. Run `nectar test` against the example app for the relevant keyword to verify the contract is satisfied.

Provider modules can call any external HTTP API or SDK. They run in the JavaScript syscall layer, not in WASM, so they have full access to `fetch`, browser APIs, and Node SDKs (when running under SSR or wasmtime).

## Provider-Agnostic Namespaces

Some keyword domains have multiple competing vendors with different APIs. Nectar exposes a vendor-agnostic namespace that providers fulfill in their own way:

- **`mp::*`** — miniprogram namespace. `mp::tradePay`, `mp::login`, `mp::shareToFriends` map to whichever super-app provider is configured (Alipay first, more to follow).
- **`banking::*`** — Moov and Plaid both fulfill the `banking` interface, with different account-linking flows handled internally by the provider.
- **`payment::*`** — Stripe and Moov both fulfill the `payment` interface, with `payment::charge` semantics translated to whichever vendor is selected.

This is the same pattern as filesystem abstractions in OS kernels: the application asks for an action, the kernel routes to the configured driver.

## Testing Providers

Each provider ships with a contract test in `compiler/tests/providers/` that verifies the exported function set matches the stdlib interface. CI fails if a provider drifts from the contract.

## Why Providers Are Separate from the Compiler

- **Vendor lock-in is a configuration concern, not a language concern.** Application code stays portable.
- **Providers can ship out-of-band.** Adding Square as a third payment provider does not require a compiler release.
- **Audit and substitution.** Providers can be swapped during incident response (Stripe outage → fall back to Moov) without redeploying the application.
