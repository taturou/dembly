# Image Base example

Run the following commands from this directory. They build the fixture Base image, create the `hello` Card, validate and lock the Deck, then execute a non-interactive command in a temporary Runtime.

```bash
docker build -t dembly-fixture-base:local ../../tests/fixtures/image-base
dembly card build ../../tests/fixtures/hello-card/rootfs ./cards \
  --name hello --version 1 --mount-target /opt/dembly/cards/hello \
  --path-prepend bin --non-interactive
dembly validate
dembly lock
dembly run -- /bin/true
```
