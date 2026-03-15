// providers/moov.js — Moov payment provider for Nectar
// Implements payment.mount/create_element/confirm using pure REST via fetch.
// No vendor SDK — no script injection. Proves the provider model works without JS SDKs.
// Loaded when `provider: "moov"` is declared.
//
// Usage: include this file alongside core.js in the build output.
// The build system does this automatically based on `required_providers`.

export function register(wasmImports, R) {
  wasmImports.payment.mount = function(elId, keyPtr, keyLen, cbIdx) {
    // Moov uses pure REST — no SDK to load. Store the API key and signal ready.
    const key = R.__getString(keyPtr, keyLen);
    R.__cbData(cbIdx, R.__registerObject({ type: 'moov', apiKey: key, elId: elId }));
  };

  wasmImports.payment.create_element = function(moovId, elId, cbIdx) {
    // Moov has no card element widget — the form is WASM-rendered HTML.
    // Return a handle that confirm() will use to collect form data.
    const el = R.__getElement(elId);
    R.__cbData(cbIdx, R.__registerObject({ type: 'moov_element', container: el }));
  };

  wasmImports.payment.confirm = function(moovId, elementId, secretPtr, secretLen, cbIdx) {
    const moov = R.__getObject(moovId);
    const secret = R.__getString(secretPtr, secretLen);
    fetch('https://api.moov.io/v1/payments', {
      method: 'POST',
      headers: { 'Authorization': 'Bearer ' + moov.apiKey, 'Content-Type': 'application/json' },
      body: secret,
    })
      .then(r => r.text())
      .then(text => R.__cbData(cbIdx, R.__allocString(text)))
      .catch(() => R.__cbData(cbIdx, 0));
  };
}
