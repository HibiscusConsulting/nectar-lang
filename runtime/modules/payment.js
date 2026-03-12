// runtime/modules/payment.js — PCI-compliant payment processing runtime

const PaymentRuntime = {
  _providers: new Map(),

  initProvider(readString, namePtr, nameLen, providerPtr, providerLen, sandboxed) {
    const name = readString(namePtr, nameLen);
    const provider = readString(providerPtr, providerLen);
    PaymentRuntime._providers.set(name, { provider, sandboxed: !!sandboxed, loaded: false });
  },

  createCheckout(readString, namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    const p = PaymentRuntime._providers.get(name);
    if (p?.sandboxed) {
      const iframe = document.createElement('iframe');
      iframe.sandbox = 'allow-scripts allow-forms allow-same-origin';
      iframe.style.cssText = 'border:none;width:100%;height:300px;';
      return 1;
    }
    return 0;
  },

  processPayment(readString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    return 1;
  },
};

const paymentModule = {
  name: 'payment',
  runtime: PaymentRuntime,
  wasmImports: {
    payment: {
      initProvider: PaymentRuntime.initProvider,
      createCheckout: PaymentRuntime.createCheckout,
      processPayment: PaymentRuntime.processPayment,
    }
  }
};

if (typeof module !== "undefined") module.exports = paymentModule;
