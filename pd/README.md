# Placement Driver

## Getting started

### Dependencies

- [Make](https://www.gnu.org/software/make/)
- [Docker](https://www.docker.com/)

*Note: Golang is not required to build the project, as the build is done inside a Docker container.*

### Building

```sh
make
```

### Running

```sh
./bin/${OS}_${ARCH}/pd
```
For more options, run `./bin/${OS}_${ARCH}/pd --help`.

## Contributing

Please ensure that your change passes static analysis:
- `make lint` to run linters.
- `make test` to run unit tests.

For more options, run `make help`.
