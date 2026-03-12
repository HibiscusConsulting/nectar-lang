// runtime/modules/channel.js — WebSocket channel runtime

const ChannelRuntime = {
  _wsChannels: new Map(),

  connect(readString, namePtr, nameLen, urlPtr, urlLen) {
    const name = readString(namePtr, nameLen);
    const url = readString(urlPtr, urlLen);
    const ch = { name, url, ws: null, reconnect: true, heartbeatId: null, reconnectDelay: 1000 };

    const open = () => {
      ch.ws = new WebSocket(url);
      ch.ws.onopen = () => {
        ch.reconnectDelay = 1000;
        if (ch.onConnect) ch.onConnect();
      };
      ch.ws.onmessage = (e) => {
        if (ch.onMessage) ch.onMessage(e.data);
      };
      ch.ws.onclose = () => {
        if (ch.onDisconnect) ch.onDisconnect();
        if (ch.reconnect) {
          setTimeout(open, Math.min(ch.reconnectDelay *= 1.5, 30000));
        }
      };
      ch.ws.onerror = () => ch.ws.close();
    };

    ChannelRuntime._wsChannels.set(name, ch);
    open();
  },

  send(readString, namePtr, nameLen, dataPtr, dataLen) {
    const name = readString(namePtr, nameLen);
    const data = readString(dataPtr, dataLen);
    const ch = ChannelRuntime._wsChannels.get(name);
    if (ch?.ws?.readyState === WebSocket.OPEN) {
      ch.ws.send(data);
    }
  },

  close(readString, namePtr, nameLen) {
    const name = readString(namePtr, nameLen);
    const ch = ChannelRuntime._wsChannels.get(name);
    if (ch) {
      ch.reconnect = false;
      ch.ws?.close();
      ChannelRuntime._wsChannels.delete(name);
    }
  },

  setReconnect(readString, namePtr, nameLen, enabled) {
    const name = readString(namePtr, nameLen);
    const ch = ChannelRuntime._wsChannels.get(name);
    if (ch) ch.reconnect = !!enabled;
  },
};

const channelModule = {
  name: 'channel',
  runtime: ChannelRuntime,
  wasmImports: {
    channel: {
      connect: ChannelRuntime.connect,
      send: ChannelRuntime.send,
      close: ChannelRuntime.close,
      setReconnect: ChannelRuntime.setReconnect,
    }
  }
};

if (typeof module !== "undefined") module.exports = channelModule;
