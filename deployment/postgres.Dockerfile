FROM postgres:18

COPY deployment/init-user.sh /docker-entrypoint-initdb.d/01-init-user.sh

RUN chmod +x /docker-entrypoint-initdb.d/01-init-user.sh
