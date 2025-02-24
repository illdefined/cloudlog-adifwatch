## Synopsis

This tool watches an [ADIF](https://adif.org/) log file for changes using the native filesystem
notification mechanism and uploads it to a [Cloudlog](https://www.magicbug.co.uk/cloudlog/)
instance through the [QSO API](https://github.com/magicbug/Cloudlog/wiki/API#apiqso).

## Usage

On Linux systems the recommended way to use this tool is through the provided systemd unit. It
should be installed to `~/.config/systemd/user/`, the URL of the Cloudlog instance adjusted
appropriately and then started with `systemctl --user start cloudlog-adifwatch.service`. To start it
automatically when the user logs in, enable it with `systemctl --user enable
cloudlog-adifwatch.service`.

By default the systemd unit expects the Cloudlog API key to be available in a file at
`~/.config/cloudlog-adifwatch/key.txt`, uses station profile ID 1 and watches the WSJT-X ADIF log
file at `~/.local/share/WSJT-X/wsjtx_log.adi`. These paths can be adjusted as needed in the unit
file.

Alternatively, this tool can be started manually.

### Example

```
cloudlog-adifwatch https://fernschreibstelle.de ~/.config/cloudlog-adifwatch/key.txt 1 ~/.local/share/WSJT-X/wsjtx_log.adi
```

## Implementation notes

The log is split into chunks of one ore more complete records which are then uploaded individually.
Partial writes to the log file are handled gracefully and only complete records are uploaded.

The log file is kept open for reading and assumed to be appended to only. Truncation of the log file
or overwriting of data already written to the log file will likely result in undesired behaviour.

## Intellectual property

This work is dedicated to the public domain under the terms of the
[CC0 1.0 licence](https://creativecommons.org/publicdomain/zero/1.0/).

The author holds no patent rights related to this work.
