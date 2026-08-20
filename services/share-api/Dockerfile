FROM node:24-alpine
WORKDIR /app
COPY package.json ./
COPY src ./src
COPY public ./public
ENV NODE_ENV=production NARRAHUB_SHARE_PORT=8787 NARRAHUB_SHARE_DATA_DIR=/data
EXPOSE 8787
VOLUME ["/data"]
RUN mkdir -p /data && chown node:node /data
USER node
CMD ["node", "src/server.mjs"]
