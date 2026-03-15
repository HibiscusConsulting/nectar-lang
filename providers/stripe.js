// providers/stripe.js — Stripe payment provider for Nectar
// Overrides the generic payment.mount/create_element/confirm stubs in core.js
// with Stripe.js SDK calls. Loaded when `provider: "stripe"` is declared.
//
// Usage: include this file alongside core.js in the build output.
// The build system does this automatically based on `required_providers`.

export function register(wasmImports, R) {
  wasmImports.payment.mount = function(elId, keyPtr, keyLen, cbIdx) {
    const s = document.createElement('script');
    s.src = 'https://js.stripe.com/v3/';
    s.onload = () => R.__cbData(cbIdx, R.__registerObject(Stripe(R.__getString(keyPtr, keyLen))));
    s.onerror = () => R.__cbData(cbIdx, 0);
    document.head.appendChild(s);
  };

  wasmImports.payment.create_element = function(stripeId, elId, cbIdx) {
    const card = R.__getObject(stripeId).elements().create('card');
    card.mount(R.__getElement(elId));
    R.__cbData(cbIdx, R.__registerObject(card));
  };

  wasmImports.payment.confirm = function(stripeId, cardId, secretPtr, secretLen, cbIdx) {
    R.__getObject(stripeId).confirmCardPayment(R.__getString(secretPtr, secretLen),
      { payment_method: { card: R.__getObject(cardId) } })
      .then(r => R.__cbData(cbIdx, R.__allocString(r.paymentIntent ? r.paymentIntent.id : '')))
      .catch(() => R.__cbData(cbIdx, 0));
  };
}
