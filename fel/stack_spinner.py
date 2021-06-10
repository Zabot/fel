import time
import sys
import threading


class ThreadGroup:
    def __enter__(self):
        self.pending_threads = []
        self.exceptions = {}
        return self

    def __exit__(self, *args):
        exception = None
        for tid, thread in enumerate(self.pending_threads):
            thread.join()
            if tid in self.exceptions:
                exception = self.exceptions[tid]
                # raise self.exceptions[tid]

        if exception is not None:
            raise Exception("Exception in thread") from exception

    def do(self, function, *args, **kwargs):
        tid = len(self.pending_threads)

        def catch_errors(*args, **kwargs):
            try:
                function(*args, **kwargs)
            except Exception as ex:
                self.exceptions[tid] = ex

        t = threading.Thread(target=catch_errors, args=args, kwargs=kwargs)
        t.start()

        self.pending_threads.append(t)
        return t


class Spinner:
    def __init__(self, label):
        self.pattern = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
        self.interval = 0.1
        self.ns_interval = self.interval * 1000000000
        self.start = time.monotonic_ns()
        self.label = label
        self.spinning = True
        self.print_queue = []

    def __str__(self):
        elapsed = time.monotonic_ns() - self.start
        frame = int(elapsed / self.ns_interval)
        glyph = self.pattern[frame % len(self.pattern)]
        return str(self.label).format(spinner=glyph)

    def spin(self, clear=True):
        """Take over the current thread to display the spinner."""
        # Hide cursor
        sys.stdout.write("\033[?25l")

        spinner_lines = 0
        while self.spinning:
            time.sleep(self.interval)

            if spinner_lines > 0:
                sys.stdout.write(f"\033[{spinner_lines}F")
            spinner_lines = 0

            while self.print_queue:
                args = self.print_queue.pop(0)
                for line in " ".join(map(str, args)).split("\n"):
                    sys.stdout.write("\033[K")
                    sys.stdout.write(line + "\n")

            lines = str(self).split("\n")
            for line in lines:
                spinner_lines += 1
                sys.stdout.write("\033[K")
                sys.stdout.write(line + "\n")

        # Clear all the spinner lines before exiting
        if clear:
            for _ in range(spinner_lines):
                sys.stdout.write("\033[1F")
                sys.stdout.write("\033[K")

        # Unhide cursor
        sys.stdout.write("\033[?25h")

    def print(self, *args):
        """Print something to stdout while the spinner is spinning without racing."""
        self.print_queue.append(args)

    def stop(self):
        """Stop the spinner and return from spin"""
        self.spinning = False

    def __enter__(self):
        self.thread = threading.Thread(target=self.spin)
        self.thread.start()
        return self

    def __exit__(self, exc_type, exc_value, exc_traceback):
        self.stop()
        self.thread.join()
        sys.stdout.write("\n")

        # print(exc_traceback)
        # print(exc_value)
