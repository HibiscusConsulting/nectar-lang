// runtime/modules/db.js — IndexedDB abstraction runtime

const DbRuntime = {
  _dbs: new Map(),

  async open(readString, namePtr, nameLen, version) {
    const name = readString(namePtr, nameLen);
    return new Promise((resolve, reject) => {
      const req = indexedDB.open(name, version);
      req.onupgradeneeded = (e) => {
        const db = e.target.result;
        DbRuntime._dbs.set(name, db);
      };
      req.onsuccess = (e) => {
        DbRuntime._dbs.set(name, e.target.result);
        resolve(1);
      };
      req.onerror = () => reject(0);
    });
  },

  put(readString, dbPtr, dbLen, storePtr, storeLen, dataPtr, dataLen) {
    const dbName = readString(dbPtr, dbLen);
    const storeName = readString(storePtr, storeLen);
    const data = JSON.parse(readString(dataPtr, dataLen));
    const db = DbRuntime._dbs.get(dbName);
    if (db) {
      const tx = db.transaction(storeName, 'readwrite');
      tx.objectStore(storeName).put(data);
    }
  },

  get(readString, dbPtr, dbLen, storePtr, storeLen) {
    const dbName = readString(dbPtr, dbLen);
    const storeName = readString(storePtr, storeLen);
    const db = DbRuntime._dbs.get(dbName);
    if (!db) return 0;
    return new Promise((resolve) => {
      const tx = db.transaction(storeName, 'readonly');
      const req = tx.objectStore(storeName).getAll();
      req.onsuccess = () => resolve(req.result);
    });
  },

  delete(readString, dbPtr, dbLen, storePtr, storeLen) {
    const dbName = readString(dbPtr, dbLen);
    const storeName = readString(storePtr, storeLen);
    const db = DbRuntime._dbs.get(dbName);
    if (db) {
      const tx = db.transaction(storeName, 'readwrite');
      tx.objectStore(storeName).clear();
    }
  },

  query(readString, dbPtr, dbLen, storePtr, storeLen) {
    const dbName = readString(dbPtr, dbLen);
    const storeName = readString(storePtr, storeLen);
    const db = DbRuntime._dbs.get(dbName);
    if (!db) return 0;
    return new Promise((resolve) => {
      const tx = db.transaction(storeName, 'readonly');
      const req = tx.objectStore(storeName).getAll();
      req.onsuccess = () => resolve(req.result);
    });
  },
};

const dbModule = {
  name: 'db',
  runtime: DbRuntime,
  wasmImports: {
    db: {
      open: DbRuntime.open,
      put: DbRuntime.put,
      get: DbRuntime.get,
      delete: DbRuntime.delete,
      query: DbRuntime.query,
    }
  }
};

if (typeof module !== "undefined") module.exports = dbModule;
