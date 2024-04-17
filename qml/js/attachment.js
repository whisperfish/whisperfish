function isPlaylist(filename) {
        return /\.m3u$/.test(filename) || /\.m3u8$/.test(filename) || /\.pls$/.test(filename) || /\.asx$/.test(filename) || /\.wpl$/.test(filename) || /\.cue$/.test(filename)
    }
