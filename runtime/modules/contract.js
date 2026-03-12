// runtime/modules/contract.js — API boundary validation runtime

const ContractRuntime = {
  _contracts: new Map(),

  registerSchema(readString, namePtr, nameLen, hashPtr, hashLen, schemaPtr, schemaLen) {
    const name = readString(namePtr, nameLen);
    const hash = readString(hashPtr, hashLen);
    const schemaJson = readString(schemaPtr, schemaLen);
    try {
      const schema = JSON.parse(schemaJson);
      ContractRuntime._contracts.set(name, { hash, schema, name });
    } catch (e) {
      console.warn(`[Nectar Contract] Failed to parse schema for ${name}:`, e);
      ContractRuntime._contracts.set(name, { hash, schema: {}, name });
    }
  },

  validate(readString, pendingFetches, responseHandle, _responseUnused, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    if (!ContractRuntime._contracts.has(name)) {
      console.warn(`[Nectar Contract] Unknown contract: ${name}`);
      return responseHandle;
    }
    const contract = ContractRuntime._contracts.get(name);

    const pending = pendingFetches.get(responseHandle);
    if (!pending || !pending._body) return responseHandle;

    let body;
    try {
      body = typeof pending._body === 'string' ? JSON.parse(pending._body) : pending._body;
    } catch {
      const err = new Error(`[Nectar Contract] ${name}: response is not valid JSON`);
      err.contract = name;
      err.type = 'contract_parse_error';
      throw err;
    }

    const missing = [];
    const wrongType = [];
    for (const [field, spec] of Object.entries(contract.schema)) {
      if (!(field in body)) {
        if (!spec.nullable) missing.push(field);
        continue;
      }
      const value = body[field];
      const actual = Array.isArray(value) ? 'array' : typeof value;
      const expected = spec.type;
      if (value === null && spec.nullable) continue;
      if (expected === 'integer' && (typeof value !== 'number' || !Number.isInteger(value))) {
        wrongType.push({ field, expected: 'integer', actual });
      } else if (expected === 'number' && typeof value !== 'number') {
        wrongType.push({ field, expected: 'number', actual });
      } else if (expected === 'string' && typeof value !== 'string') {
        wrongType.push({ field, expected: 'string', actual });
      } else if (expected === 'boolean' && typeof value !== 'boolean') {
        wrongType.push({ field, expected: 'boolean', actual });
      } else if (expected === 'array' && !Array.isArray(value)) {
        wrongType.push({ field, expected: 'array', actual });
      }
    }

    if (missing.length > 0 || wrongType.length > 0) {
      const details = [];
      if (missing.length) details.push(`missing fields: ${missing.join(', ')}`);
      if (wrongType.length) details.push(
        `type mismatches: ${wrongType.map(w => `${w.field} (expected ${w.expected}, got ${w.actual})`).join(', ')}`
      );
      const err = new Error(
        `[Nectar Contract] ${name}@${contract.hash}: boundary validation failed — ${details.join('; ')}`
      );
      err.contract = name;
      err.hash = contract.hash;
      err.missing = missing;
      err.wrongType = wrongType;
      err.type = 'contract_mismatch';
      throw err;
    }

    return responseHandle;
  },

  getHash(readString, writeString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    if (!ContractRuntime._contracts.has(name)) return 0;
    const hash = ContractRuntime._contracts.get(name).hash;
    const headerVal = `${name}@${hash}`;
    return writeString(headerVal);
  },
};

const contractModule = {
  name: 'contract',
  runtime: ContractRuntime,
  wasmImports: {
    contract: {
      registerSchema: ContractRuntime.registerSchema,
      validate: ContractRuntime.validate,
      getHash: ContractRuntime.getHash,
    }
  }
};

if (typeof module !== "undefined") module.exports = contractModule;
