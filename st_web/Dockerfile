# ═══════════════════════════════════════════════════════
# HARNESS 官网 — 多阶段构建（node 构建 → nginx 静态托管）
# 构建：docker build -t harness-web .
# 运行：docker run -p 8080:80 harness-web
# ═══════════════════════════════════════════════════════

# ── 阶段 1：构建 ──
FROM node:20-alpine AS builder
WORKDIR /app

# 依赖缓存层
COPY package.json package-lock.json ./
RUN npm ci

# 源码 + 构建
COPY . .
ARG NEXT_PUBLIC_SITE_URL=https://st-control.dev
ENV NEXT_PUBLIC_SITE_URL=$NEXT_PUBLIC_SITE_URL
RUN npm run build

# ── 阶段 2：nginx 静态托管 ──
FROM nginx:1.27-alpine AS runner
COPY docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/out /usr/share/nginx/html

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
  CMD wget -qO- http://127.0.0.1/zh/ >/dev/null || exit 1

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
