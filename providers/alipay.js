// providers/alipay.js — Alipay payment provider for Nectar
// Implements payment.mount/create_element/confirm/key_exchange using pure REST.
// No vendor SDK — RSA2 signing happens in WASM, this is just the fetch bridge.
// Loaded when `provider: "alipay"` or `payment_provider: "alipay"` is declared.
//
// Supports both domestic (openapi.alipay.com) and sandbox (openapi-sandbox.dl.alipaydev.com).
// Usage: include this file alongside core.js in the build output.

export function register(wasmImports, R) {
  let gateway = 'https://openapi-sandbox.dl.alipaydev.com/gateway.do';
  let appId = '';

  wasmImports.payment.mount = function(elId, keyPtr, keyLen, cbIdx) {
    appId = R.__getString(keyPtr, keyLen);
    R.__cbData(cbIdx, R.__registerObject({ type: 'alipay', appId, gateway }));
  };

  wasmImports.payment.create_element = function(alipayId, elId, cbIdx) {
    R.__cbData(cbIdx, R.__registerObject({ type: 'alipay_element', container: R.__getElement(elId) }));
  };

  wasmImports.payment.confirm = function(alipayId, elementId, dataPtr, dataLen, cbIdx) {
    const body = R.__getString(dataPtr, dataLen);
    fetch(gateway, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body })
      .then(r => r.text())
      .then(text => R.__cbData(cbIdx, R.__allocString(text)))
      .catch(() => R.__cbData(cbIdx, 0));
  };

  wasmImports.payment.key_exchange = function(pubKeyPtr, pubKeyLen, cbIdx) {
    const pubKey = R.__getString(pubKeyPtr, pubKeyLen);
    fetch(gateway, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: 'method=nectar.key.exchange&pub_key=' + encodeURIComponent(pubKey) })
      .then(r => r.text())
      .then(text => R.__cbData(cbIdx, R.__allocString(text)))
      .catch(() => R.__cbData(cbIdx, 0));
  };
}
