FROM yfractal0/sdb

COPY ./ /sdb
WORKDIR /sdb
RUN --mount=type=ssh /bin/bash -c "source /etc/profile.d/rbenv.sh && bundle install"
RUN --mount=type=ssh /bin/bash -c "source /etc/profile.d/rbenv.sh && bundle exec rake compile"

CMD ["bash"]
