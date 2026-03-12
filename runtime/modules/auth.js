// runtime/modules/auth.js — Authentication runtime (OAuth, JWT, sessions)

const AuthRuntime = {
  _config: null,
  _user: null,
  _token: null,

  initAuth(readString, namePtr, nameLen, configPtr, configLen) {
    const name = readString(namePtr, nameLen);
    const config = JSON.parse(readString(configPtr, configLen));
    AuthRuntime._config = { name, ...config };
  },

  login(readString, providerPtr, providerLen) {
    const provider = readString(providerPtr, providerLen);
    const config = AuthRuntime._config;
    if (config?.providers?.[provider]) {
      const p = config.providers[provider];
      const authUrl = `https://accounts.google.com/o/oauth2/v2/auth?client_id=${p.client_id}&scope=${p.scopes.join('+')}&response_type=code&redirect_uri=${location.origin}/auth/callback`;
      location.href = authUrl;
    }
    return 0;
  },

  logout(readString, namePtr, nameLen) {
    AuthRuntime._user = null;
    AuthRuntime._token = null;
    document.cookie = 'nectar_session=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
  },

  getUser() { return AuthRuntime._user; },
  isAuthenticated() { return AuthRuntime._user ? 1 : 0; },
};

const authModule = {
  name: 'auth',
  runtime: AuthRuntime,
  wasmImports: {
    auth: {
      initAuth: AuthRuntime.initAuth,
      login: AuthRuntime.login,
      logout: AuthRuntime.logout,
      getUser: AuthRuntime.getUser,
      isAuthenticated: AuthRuntime.isAuthenticated,
    }
  }
};

if (typeof module !== "undefined") module.exports = authModule;
