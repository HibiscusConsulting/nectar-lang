# buildnectar.com — static site served by nginx with gzip + clean URLs.
# website/dist/ is pre-built locally via scripts/build_docs_html.py.

FROM nginx:alpine

COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY dist/ /usr/share/nginx/html/

EXPOSE 8080
CMD ["nginx", "-g", "daemon off;"]
