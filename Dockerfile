FROM nginx:alpine

# Remove default config
RUN rm /etc/nginx/conf.d/default.conf

# Add our config template (PORT_PLACEHOLDER gets replaced at startup)
COPY nginx.conf /etc/nginx/conf.d/app.conf.template

# Copy the app
COPY index.html /usr/share/nginx/html/
COPY worker.js /usr/share/nginx/html/
COPY wasm-solver/pkg/ /usr/share/nginx/html/wasm-solver/pkg/

# At runtime, substitute $PORT into nginx config and start nginx.
# Railway injects PORT; default to 80 if not set.
CMD sh -c "sed 's/PORT_PLACEHOLDER/'\"${PORT:-80}\"'/' /etc/nginx/conf.d/app.conf.template > /etc/nginx/conf.d/default.conf && nginx -g 'daemon off;'"
