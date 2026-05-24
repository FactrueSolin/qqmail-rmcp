set shell := ["bash", "-lc"]

default:
    @just --list

build:
    @bash ./just/qqmail-macos-service.sh build

deploy:
    @bash ./just/qqmail-macos-service.sh deploy

redeploy:
    @bash ./just/qqmail-macos-service.sh redeploy

start:
    @bash ./just/qqmail-macos-service.sh start


stop:
    @bash ./just/qqmail-macos-service.sh stop

restart:
    @bash ./just/qqmail-macos-service.sh restart

delete:
    @bash ./just/qqmail-macos-service.sh delete

status:
    @bash ./just/qqmail-macos-service.sh status

logs *args:
    @bash ./just/qqmail-macos-service.sh logs {{args}}

test:
    @bash ./tests/imap_regression.sh
    @bash ./tests/oauth_provider_acceptance.sh

plist:
    @bash ./just/qqmail-macos-service.sh plist
