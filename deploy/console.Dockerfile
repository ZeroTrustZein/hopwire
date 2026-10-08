# syntax=docker/dockerfile:1
# AegisGraph investigator console. Build from the repo root:
#   docker build -f deploy/console.Dockerfile -t aegisgraph/console .

FROM node:24-alpine AS build
WORKDIR /app
COPY console/package.json console/package-lock.json ./
RUN npm ci
COPY console/ ./
# Empty = same origin: nginx below proxies /v1 to the engine.
ARG VITE_API_URL=
ENV VITE_API_URL=${VITE_API_URL}
RUN npm run build

FROM nginx:alpine
# nginx's entrypoint renders /etc/nginx/templates/*.template with envsubst
# (only variables defined in the environment, so nginx's own $uri etc. survive).
ENV AEGIS_ENGINE_URL=http://engine:8080
COPY deploy/nginx.conf /etc/nginx/templates/default.conf.template
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=5 \
  CMD wget -q -O /dev/null http://127.0.0.1/healthz || exit 1
