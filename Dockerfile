FROM nginx:alpine

# Correct MIME type for WebAssembly files
RUN echo 'types { application/wasm wasm; }' > /etc/nginx/conf.d/wasm-mime.conf

# Copy the app into nginx's default serve directory
COPY index.html /usr/share/nginx/html/
COPY worker.js /usr/share/nginx/html/
COPY wasm-solver/pkg/ /usr/share/nginx/html/wasm-solver/pkg/

EXPOSE 80
