default:
    @just --list

build:
    @bash ./just/qqmail-macos-service.sh build

deploy:
    @bash ./just/qqmail-macos-service.sh deploy

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

plist:
    @bash ./just/qqmail-macos-service.sh plist
