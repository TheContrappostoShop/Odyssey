#!/usr/bin/env python
import argparse
import requests
import json

def start(url, location, filename):
    return
    #send request

def cli_start(args):
    return start(args.url, args.location, args.filename)

def stop(url):
    return
    #stop request

def cli_stop(args):
    return stop(args.url)

def pause(url):
    return
    #pause request

def cli_pause(args):
    return pause(args.url)
    
def resume(url):
    return
    #resume request

def cli_resume(args):
    return resume(args.url)
    
def status(url):
    response = requests.get(f"{url}/status")
    response.raise_for_status()
    try:
        return json.dumps(response.json(), indent=4)
    except:
        return response.status_code

def cli_status(args):
    return status(args.url)


def main():
    parser = argparse.ArgumentParser(
        description="This script provides an easier way to interact with the \
            Odyssey API from a local context, such as Klipper Macros or the \
            command line."
    )

    parser.add_argument('-u', '--url', default="http://127.0.0.1:12357")

    subparsers = parser.add_subparsers(
        required=True,
        title='API Endpoints',
        description='Valid CLI Endpoints',
        dest='endpoint',
        metavar=''
    )

    start_parser = subparsers.add_parser(
        'start',
        help='Start printing the specified file'
    )
    start_parser.add_argument('location')
    start_parser.add_argument('filename')
    start_parser.set_defaults(func=cli_start)
    
    stop_parser = subparsers.add_parser(
        'stop',
        help='Stop the current print (at the end of the current layer)'
    )
    stop_parser.set_defaults(func=cli_stop)

    pause_parser = subparsers.add_parser(
        'pause',
        help='Pause the current print (at the end of the current layer)'
    )
    pause_parser.set_defaults(func=cli_pause)

    resume_parser = subparsers.add_parser(
        'resume',
        help='Resume a previously paused print'
    )
    resume_parser.set_defaults(func=cli_resume)

    status_parser = subparsers.add_parser(
        'status',
        help='Return the current status from Odyssey'
    )
    status_parser.set_defaults(func=cli_status)

    args = parser.parse_args()

    print(args.func(args))

if __name__ == "__main__":
    main()