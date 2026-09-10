# Image Base example

`dembly card build ../../tests/fixtures/hello-card/rootfs ./cards --name hello --version 1 --mount-target /opt/dembly/cards/hello --path-prepend bin --non-interactive` で Card artifact を作成します。

`dembly lock`、`dembly run -- hello` の順で使用します。
