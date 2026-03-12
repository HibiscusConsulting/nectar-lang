// runtime/modules/upload.js — Resumable chunked file upload runtime

const UploadRuntime = {
  _uploads: new Map(),

  init(readString, namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    UploadRuntime._uploads.set(name, { config, active: null });
  },

  start(readString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    const upload = UploadRuntime._uploads.get(name);
    if (!upload) return 0;
    const input = document.createElement('input');
    input.type = 'file';
    if (upload.config.accept) input.accept = upload.config.accept.join(',');
    input.onchange = async () => {
      const file = input.files[0];
      if (!file) return;
      if (upload.config.max_size && file.size > upload.config.max_size) {
        if (upload.config.onError) upload.config.onError('File too large');
        return;
      }
      const xhr = new XMLHttpRequest();
      xhr.upload.onprogress = (e) => {
        if (e.lengthComputable && upload.config.onProgress) {
          upload.config.onProgress(Math.round(e.loaded / e.total * 100));
        }
      };
      xhr.onload = () => { if (upload.config.onComplete) upload.config.onComplete(xhr.response); };
      xhr.onerror = () => { if (upload.config.onError) upload.config.onError(xhr.statusText); };
      xhr.open('POST', upload.config.endpoint);
      const form = new FormData();
      form.append('file', file);
      xhr.send(form);
    };
    input.click();
    return 1;
  },

  cancel(readString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    const upload = UploadRuntime._uploads.get(name);
    if (upload?.active) upload.active.abort();
  },
};

const uploadModule = {
  name: 'upload',
  runtime: UploadRuntime,
  wasmImports: {
    upload: {
      init: UploadRuntime.init,
      start: UploadRuntime.start,
      cancel: UploadRuntime.cancel,
    }
  }
};

if (typeof module !== "undefined") module.exports = uploadModule;
