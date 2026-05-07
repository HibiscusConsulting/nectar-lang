# buildnectar.com — static site served by nginx.
# website/dist/ is pre-built locally via scripts/build_docs_html.py
# (and the marketing index.html is hand-curated).

FROM nginx:alpine

COPY dist/ /usr/share/nginx/html/

# Nginx config: clean URLs for /docs (try .html extension), SPA fallback elsewhere
RUN printf 'server {\n\
    listen 8080;\n\
    root /usr/share/nginx/html;\n\
    index index.html;\n\
    \n\
    # Clean URLs for documentation: /docs/foo -> /docs/foo.html\n\
    location /docs/ {\n\
        try_files $uri $uri.html $uri/index.html /docs/index.html;\n\
    }\n\
    \n\
    # Top-level routes fall back to the marketing homepage\n\
    location / {\n\
        try_files $uri $uri.html $uri/ /index.html;\n\
    }\n\
    \n\
    # Long-cache immutable assets\n\
    location ~* \\.(wasm|js|css|png|jpg|jpeg|svg|ico|woff2?)$ {\n\
        expires 1y;\n\
        add_header Cache-Control "public, immutable";\n\
    }\n\
}\n' > /etc/nginx/conf.d/default.conf

EXPOSE 8080
CMD ["nginx", "-g", "daemon off;"]
