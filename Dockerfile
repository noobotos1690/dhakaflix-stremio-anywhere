# DhakaFlix relay image.
#
# Runs the addon + streaming relay. It must be deployed on a host that can
# route to 172.16.50.0/24 (a BDIX-peered VPS or your own ISP connection).

FROM node:22-alpine

ENV NODE_ENV=production \
    PORT=7001 \
    HOST=0.0.0.0

WORKDIR /app

# Dependencies first so code changes don't bust the layer cache.
COPY package.json package-lock.json ./
RUN npm ci --omit=dev && npm cache clean --force

COPY addon.json index.js server.js ./
COPY src ./src

RUN adduser -D -H -u 10001 relay && chown -R relay:relay /app
USER relay

EXPOSE 7001

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- "http://127.0.0.1:${PORT}/health" > /dev/null || exit 1

CMD ["node", "index.js"]
