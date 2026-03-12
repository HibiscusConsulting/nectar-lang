// runtime/modules/pdf.js — PDF generation + IO/download runtime

const PdfRuntime = {
  _docs: new Map(),
  _nextId: 1,

  create(readString, namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = configLen > 0 ? JSON.parse(readString(configPtr, configLen)) : {};
    const id = PdfRuntime._nextId++;
    PdfRuntime._docs.set(id, { name, config, content: null });
    return id;
  },

  render(readString, handleId, htmlPtr, htmlLen) {
    const html = readString(htmlPtr, htmlLen);
    const doc = PdfRuntime._docs.get(handleId);
    if (doc) {
      doc.content = html;
      const iframe = document.createElement('iframe');
      iframe.style.cssText = 'position:absolute;left:-9999px;width:0;height:0;';
      document.body.appendChild(iframe);
      const iframeDoc = iframe.contentDocument || iframe.contentWindow.document;
      iframeDoc.open();
      const pageSize = doc.config.pageSize || 'A4';
      const orientation = doc.config.orientation || 'portrait';
      iframeDoc.write(`
        <html>
        <head>
          <style>
            @page { size: ${pageSize} ${orientation}; margin: 1cm; }
            @media print { body { margin: 0; } }
          </style>
        </head>
        <body>${html}</body>
        </html>
      `);
      iframeDoc.close();
      doc.iframe = iframe;
    }
    return handleId;
  },
};

const IoRuntime = {
  download(readString, dataPtr, dataLen, namePtr, nameLen) {
    const data = readString(dataPtr, dataLen);
    const filename = readString(namePtr, nameLen);
    const blob = new Blob([data], { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.style.display = 'none';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  },
};

const pdfModule = {
  name: 'pdf',
  runtime: { PdfRuntime, IoRuntime },
  wasmImports: {
    pdf: {
      create: PdfRuntime.create,
      render: PdfRuntime.render,
    },
    io: {
      download: IoRuntime.download,
    }
  }
};

if (typeof module !== "undefined") module.exports = pdfModule;
