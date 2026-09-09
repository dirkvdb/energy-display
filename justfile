build:
    devenv tasks run firmware:build

test:
    devenv tasks run firmware:validate
    devenv tasks run simulator:check

sim:
    devenv tasks run simulator:run
