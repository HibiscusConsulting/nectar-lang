// providers/plaid.js — Plaid banking provider for Nectar
// Overrides the generic banking.open stub in core.js with Plaid Link SDK calls.
// Loaded when `provider: "plaid"` is declared.
//
// Usage: include this file alongside core.js in the build output.
// The build system does this automatically based on `required_providers`.

export function register(wasmImports, R) {
  wasmImports.banking.open = function(tokenPtr, tokenLen, cbIdx) {
    const s = document.createElement('script');
    s.src = 'https://cdn.plaid.com/link/v2/stable/link-initialize.js';
    s.onload = () => {
      Plaid.create({
        token: R.__getString(tokenPtr, tokenLen),
        onSuccess: (token) => R.__cbData(cbIdx, R.__allocString(token)),
        onExit: () => R.__cbData(cbIdx, 0),
      }).open();
    };
    s.onerror = () => R.__cbData(cbIdx, 0);
    document.head.appendChild(s);
  };
}
