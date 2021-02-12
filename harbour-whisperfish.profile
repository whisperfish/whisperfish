# -*- mode: sh -*-

# Firejail profile for /usr/bin/harbour-whisperfish

# x-sailjail-translation-catalog = harbour-whisperfish
# x-sailjail-translation-key-description = permission-la-data
# x-sailjail-description = Whisperfish data storage
# x-sailjail-translation-key-long-description = permission-la-data_description
# x-sailjail-long-description = Store configuration and messages

### PERMISSIONS
# x-sailjail-permission = Internet
# x-sailjail-permission = Pictures
# x-sailjail-permission = MediaIndexing
# x-sailjail-permission = Contacts
# x-sailjail-permission = Notifications
# x-sailjail-permission = Phone
# x-sailjail-permission = Privileged
# x-sailjail-permission = Mozilla

mkdir     ${HOME}/.config/harbour-whisperfish
whitelist ${HOME}/.config/harbour-whisperfish

mkdir     ${HOME}/.local/share/harbour-whisperfish
whitelist ${HOME}/.local/share/harbour-whisperfish

dbus-user.own org.whisperfish
dbus-user.own org.whisperfish.session
