# Build stage — compiles Nectar website to static files
FROM rust:1.85 AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

# For now, serve pre-built static files
# Once nectar build --target ssg works end-to-end, this becomes:
# RUN ./target/release/nectar build website/src/app.nectar --target ssg --output /dist

# Serve stage — lightweight static file server
FROM nginx:alpine

# Copy static output
COPY website/dist/ /usr/share/nginx/html/

# Nginx config for SPA routing
RUN echo 'server { \
    listen 8080; \
    root /usr/share/nginx/html; \
    index index.html; \
    location / { \
        try_files $uri $uri/ /index.html; \
    } \
    # Cache static assets \
    location ~* \.(wasm|js|css|png|jpg|svg|ico)$ { \
        expires 1y; \
        add_header Cache-Control "public, immutable"; \
    } \
}' > /etc/nginx/conf.d/default.conf

EXPOSE 8080
CMD ["nginx", "-g", "daemon off;"]
