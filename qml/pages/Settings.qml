import QtQuick 2.2
import Sailfish.Silica 1.0
import be.rubdos.whisperfish 1.0
import "../js/countries_iso_only.js" as Countries
import "../components"

Page {
    id: settingsPage
    objectName: "settingsPage"

    SystemdUserService {
        id: autostartService
        serviceName: 'harbour-whisperfish.service'
    }

    // Cache encryption state so it's only queried once from storage
    property bool encryptedDatabase: AppState.isEncrypted()
    readonly property bool isPrimaryDevice: SettingsBridge.isPrimaryDevice()

    // Triggers to send Syng Type::Configuration after closing the page
    property bool _typingIndicators: false
    property bool _readReceipts: false
    property bool _linkPreviews: false

    Component.onCompleted: {
        _typingIndicators = SettingsBridge.enable_typing_indicators
        _readReceipts = SettingsBridge.enable_read_receipts
        _linkPreviews = SettingsBridge.enable_link_previews
    }

    Component.onDestruction: {
        if (
            _typingIndicators != SettingsBridge.enable_typing_indicators ||
            _readReceipts != SettingsBridge.enable_read_receipts ||
            _linkPreviews != SettingsBridge.enable_link_previews
         ) {
            console.log("Configuration sync needed")
            ClientWorker.sendConfiguration()
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentWidth: parent.width
        contentHeight: contentColumn.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                //: Linked devices menu option
                //% "Linked Devices"
                text: qsTrId("whisperfish-settings-linked-devices-menu")
                onClicked: {
                    ClientWorker.reload_linked_devices();
                    pageStack.push(Qt.resolvedUrl("LinkedDevices.qml"));
                }
            }
            MenuItem {
                visible: false // XXX: Unimplemented
                //: Reconnect menu
                //% "Reconnect"
                text: qsTrId("whisperfish-settings-reconnect-menu")
                onClicked: {
                    ClientWorker.reconnect()
                }
            }
            MenuItem {
                //: Show own profile menu
                //% "Show my profile"
                text: qsTrId("whisperfish-settings-show-own-profile-menu")
                onClicked: pageStack.push(Qt.resolvedUrl("ProfilePage.qml"))
            }
        }

        VerticalScrollDecorator {}

        Column {
            id: contentColumn
            spacing: Theme.paddingLarge
            width: parent.width
            PageHeader {
                //: Settings page title
                //% "Settings"
                title: qsTrId("whisperfish-settings-title")
            }

            Label {
                visible: !isPrimaryDevice
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 4*Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                //: Settings page, not a primary device note
                //% "Some settings can only be changed from the primary device."
                text: qsTrId("whisperfish-settings-some-settings-locked")

                Rectangle {
                    z: -1
                    anchors.centerIn: parent
                    width: parent.width + 2*Theme.horizontalPageMargin
                    height: parent.height + 2*Theme.horizontalPageMargin + 1
                    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
                    radius: 2*Theme.horizontalPageMargin
                }
            }

            // ------ BEGIN GENERAL SETTINGS ------
            SectionHeader {
                //: Settings page general section
                //% "General"
                text: qsTrId("whisperfish-settings-general-section")
            }
            IconTextSwitch {
                enabled: isPrimaryDevice
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page use typing indicators
                //% "Enable typing indicators"
                text: qsTrId("whisperfish-settings-enable-typing-indicators")
                //: Settings page typing indicators description
                //% "See when others are typing, and let others see when you are typing, if they also have this enabled."
                description: qsTrId("whisperfish-settings-enable-typing-indicators-description")
                checked: SettingsBridge.enable_typing_indicators
                icon.source: "image://theme/icon-m-activity-messaging"
                onCheckedChanged: {
                    if(checked != SettingsBridge.enable_typing_indicators) {
                        SettingsBridge.enable_typing_indicators = checked
                    }
                }
            }
            IconTextSwitch {
                enabled: isPrimaryDevice
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page use read receipts
                //% "Enable read receipts"
                text: qsTrId("whisperfish-settings-enable-read-receipts")
                //: Settings page scale read receipts description
                //% "See when others have read your messages, and let others see when you are have read theirs, if they also have this enabled."
                description: qsTrId("whisperfish-settings-enable-read-receipts-description")
                checked: SettingsBridge.enable_read_receipts
                icon.source: "image://theme/icon-m-activity-messaging"
                onCheckedChanged: {
                    if(checked != SettingsBridge.enable_read_receipts) {
                        SettingsBridge.enable_read_receipts = checked
                    }
                }
            }
            IconTextSwitch {
                enabled: isPrimaryDevice
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page enable link previews
                //% "Link previews"
                text: qsTrId("whisperfish-settings-enable-link-previews")
                //: Settings page enable link previews description
                //% "Create and send previews of the links you send in messages. Note: Feature not yet implemented in Whisperfish."
                description: qsTrId("whisperfish-settings-enable-link-previews-description")
                checked: SettingsBridge.enable_link_previews
                icon.source: "image://theme/icon-m-website"
                onCheckedChanged: {
                    if(checked != SettingsBridge.enable_link_previews) {
                        SettingsBridge.enable_link_previews = checked
                    }
                }
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page notifications show minimum number of notifications
                //% "Minimise notifications"
                text: qsTrId("whisperfish-settings-notifications-minimise")
                //: Settings page notifications show minimum number of notifications description
                //% "If turned on, Whisperfish will suppress all but the first notification from each session."
                description: qsTrId("whisperfish-settings-notifications-minimise-description")
                checked: SettingsBridge.minimise_notify
                icon.source: "image://theme/icon-m-repeat-single"
                onCheckedChanged: {
                    if(checked != SettingsBridge.minimise_notify) {
                        SettingsBridge.minimise_notify = checked
                    }
                }
            }

            ComboBox {
                id: countryCombo
                property string _setting: SettingsBridge.country_code
                width: parent.width
                //: Settings page country code
                //% "Country Code"
                label: qsTrId("whisperfish-settings-country-code")
                //: Settings page country code description
                //% "The selected country code determines what happens when a local phone number is entered."
                description: qsTrId("whisperfish-settings-country-code-description")
                //: settings page country code selection: nothing selected
                //% "none"
                value: currentIndex < 0 ?
                           qsTrId("whisperfish-settings-country-code-empty") :
                           currentItem.iso
                currentIndex: -1
                menu: ContextMenu {
                    Repeater {
                        model: Countries.c
                        MenuItem {
                            property string names: Countries.c[index].n
                            property string iso: Countries.c[index].i
                            text: iso + " - " + names
                            Component.onCompleted: {
                                if (iso === countryCombo._setting) {
                                    countryCombo.currentIndex = index
                                }
                            }
                        }
                    }
                }
                onCurrentIndexChanged: {
                    if(
                        currentIndex > -1
                        && currentItem !== null
                        && SettingsBridge.country_code !== currentItem.iso
                    ) {
                        SettingsBridge.country_code = currentItem.iso
                    }
                }
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page save attachments
                //% "Save Attachments"
                text: qsTrId("whisperfish-settings-save-attachments")
                description:  {
                    //: Settings page save attachments description
                    //% "Attachments are stored at %1. Currently, when disabled, attachments will not work."
                    qsTrId("whisperfish-settings-save-attachments-description")
                        .arg(SettingsBridge.attachment_dir)
                }
                checked: SettingsBridge.save_attachments
                icon.source: "image://theme/icon-m-attach"
                onCheckedChanged: {
                    if(checked != SettingsBridge.save_attachments) {
                        SettingsBridge.save_attachments = checked
                    }
                }
            }
            IconTextSwitch {
                visible: false // XXX: Unimplemented
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page share contacts
                //% "Share Contacts"
                text: qsTrId("whisperfish-share-contacts-label")
                //: Share contacts description
                //% "Allow Signal to use your local contact list, to find other Signal users."
                description: qsTrId("whisperfish-share-contacts-description")
                checked: SettingsBridge.share_contacts
                icon.source: "image://theme/icon-m-file-vcard"
                onCheckedChanged: {
                    if(checked != SettingsBridge.share_contacts) {
                        SettingsBridge.share_contacts = checked
                    }
                }
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page prefer phone contacts
                //% "Prefer device contacts"
                text: qsTrId("whisperfish-settings-notifications-prefer-device-contacts")
                //: Settings page prefer phone contacts description
                //% "Prefer Sailfish OS address book contact names and avatars over Signal Profile data."
                description: qsTrId("whisperfish-settings-notifications-prefer-device-contacts-description")
                checked: SettingsBridge.prefer_device_contacts
                icon.source: "image://theme/icon-m-people"
                onCheckedChanged: {
                    if(checked != SettingsBridge.prefer_device_contacts) {
                        SettingsBridge.prefer_device_contacts = checked
                    }
                }
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page enable enter send
                //% "Return key send"
                text: qsTrId("whisperfish-settings-enable-enter-send")
                //: Settings page enable enter send description
                //% "When enabled, the return key functions as a send key. Otherwise, the return key can be used for multi-line messages."
                description: qsTrId("whisperfish-settings-enable-enter-send-description")
                checked: SettingsBridge.enable_enter_send
                icon.source: "image://theme/icon-m-enter"
                onCheckedChanged: {
                    if(checked != SettingsBridge.enable_enter_send) {
                        SettingsBridge.enable_enter_send = checked
                    }
                }
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page auto transcribe voice notes
                //% "Transcribe voice notes"
                text: qsTrId("whisperfish-transcribe-voice-notes-label")
                description: dbusSpeechInterface.available ?
                    //: Auto transcribe voice notes description, Speech Note installed
                    //% "Automatically transcribe voice notes to text upon reception using Speech Note."
                    qsTrId("whisperfish-transcribe-voice-notes-description-available") : (dbusSpeechInterface.installed ?
                    //: Auto transcribe voice notes description, Speech Note installed but not configured.
                    //% "Automatically transcribe voice notes to text upon reception. Configure an 'Auto detected' model in Speech Note to use this feature."
                    qsTrId("whisperfish-transcribe-voice-notes-description-unavailable") :
                    //: Auto transcribe voice notes description, Speech Note not installed
                    //% "Automatically transcribe voice notes to text upon reception. Install and configure an 'Auto detected' model in Speech Note to use this feature."
                    qsTrId("whisperfish-transcribe-voice-notes-description-uninstalled")
                )
                checked: SettingsBridge.transcribe_voice_notes
                enabled: dbusSpeechInterface.available
                icon.source: "image://theme/icon-m-file-note-dark"
                onCheckedChanged: {
                    if(checked != SettingsBridge.transcribe_voice_notes) {
                        SettingsBridge.transcribe_voice_notes = checked
                    }
                }
            }
            // ------ END GENERAL SETTINGS ------

            // ------ BEGIN PRIVACY SETTINGS ------
            SectionHeader {
                //: Settings page "privacy" section
                //% "Privacy"
                text: qsTrId("whisperfish-settings-privacy-section")
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page, share recipient phone number with contacts
                //% "Share phone number"
                text: qsTrId("whisperfish-settings-share-phone-number")
                //: Settings page, share recipient phone number with contacts
                //% "When enabled, your contacts can see your phone number when you message them."
                description: qsTrId("whisperfish-settings-share-phone-number-description")
                checked: SettingsBridge.share_phone_number
                icon.source: "image://theme/icon-m-dialpad"
                onCheckedChanged: {
                    if(checked!= SettingsBridge.share_phone_number) {
                        SettingsBridge.share_phone_number = checked
                    }
                }
            }
            ComboBox {
                property string _setting: SettingsBridge.notification_privacy
                width: parent.width
                //: Settings page notification privacy
                //% "Notification privacy"
                label: qsTrId("whisperfish-settings-notification-privacy")
                //: Settings page notification privacy description
                //% "Select how Whisperfish produces notifications"
                description: currentItem.description
                // Sync this in three places: the menu, here, and settings.rs
                currentIndex: ["off", "minimal", "sender-only", "complete"].indexOf(SettingsBridge.notification_privacy.toString())
                menu: ContextMenu {
                    MenuItem {
                        property string name: "off"
                        //: Settings page, turn notifications off
                        //% "Disable notifications"
                        text: qsTrId("whisperfish-settings-notifications-disable")
                        //: Settings page, turn notifications off description
                        //% "Whisperfish will not display any notification"
                        property string description: qsTrId("whisperfish-settings-notifications-disable-description")
                    }
                    MenuItem {
                        property string name: "minimal"
                        //: Settings page, minimal notifications
                        //% "Minimal notifications"
                        text: qsTrId("whisperfish-settings-notifications-minimal")
                        //: Settings page, minimal notifications description
                        //% "Notification without disclosing the sender or content of the message"
                        property string description: qsTrId("whisperfish-settings-notifications-minimal-description")
                    }
                    MenuItem {
                        property string name: "sender-only"
                        //: Settings page, sender-only notifications
                        //% "Sender-only notifications"
                        text: qsTrId("whisperfish-settings-notifications-sender-only")
                        //: Settings page, sender-only notifications description
                        //% "Notifications displaying the sender of a message, without the contents"
                        property string description: qsTrId("whisperfish-settings-notifications-sender-only-description")
                    }
                    MenuItem {
                        property string name: "complete"
                        //: Settings page, complete notifications
                        //% "Complete notifications"
                        text: qsTrId("whisperfish-settings-notifications-complete")
                        //: Settings page, sender-only notifications description
                        //% "Notifications displaying the contents and sender of a message"
                        property string description: qsTrId("whisperfish-settings-notifications-complete-description")
                    }
                }
                onCurrentIndexChanged: {
                    if(
                        currentIndex > -1
                        && currentItem !== null
                        && SettingsBridge.notification_privacy !== currentItem.name
                    ) {
                        SettingsBridge.notification_privacy = currentItem.name
                    }
                }
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page, show recipient phone number in conversation view
                //% "Show phone number"
                text: qsTrId("whisperfish-settings-show-phone-number")
                //: Settings page, show recipient phone number in conversation view description
                //% "Show the phone number of the recipient in the conversation page header."
                description: qsTrId("whisperfish-settings-show-phone-number-description")
                checked: SettingsBridge.show_phone_number
                icon.source: "image://theme/icon-m-phone"
                onCheckedChanged: {
                    if(checked!= SettingsBridge.show_phone_number) {
                        SettingsBridge.show_phone_number = checked
                    }
                }
            }
            // ------ END PRIVACY SETTINGS ------

            // ------ BEGIN BACKGROUND&STARTUP SETTINGS ------
            Column {
                spacing: Theme.paddingLarge
                width: parent.width
                visible: !AppState.isHarbour()

                SectionHeader {
                    //: Settings page startup and shutdown section
                    //% "Autostart and Background"
                    text: qsTrId("whisperfish-settings-startup-shutdown-section")
                }
                IconTextSwitch {
                    anchors.horizontalCenter: parent.horizontalCenter
                    //: Settings page enable autostart
                    //% "Autostart after boot"
                    text: qsTrId("whisperfish-settings-enable-autostart")
                    //: Settings page enable autostart description
                    //% "When enabled, Whisperfish starts automatically after each boot. If storage encryption is enabled or background-mode is off, the UI will be shown, otherwise the app starts in the background."
                    description: qsTrId("whisperfish-settings-enable-autostart-description")
                    enabled: autostartService.serviceExists
                    checked: autostartService.serviceEnabled
                    icon.source: "image://theme/icon-m-toy"
                    onCheckedChanged: {
                        if (enabled) {
                            if (checked) {
                                autostartService.enableService()
                            } else {
                                autostartService.disableService()
                            }
                        }
                    }
                }
                TextField {
                    id: passwordField
                    visible: encryptedDatabase
                    width: parent.width - 2*Theme.horizontalPageMargin
                    inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
                    validator: RegExpValidator{ regExp: /|.{6,}/ }
                    echoMode: TextInput.Password
                    //: Settings page autostart password field
                    //% "Unlock Password"
                    label: qsTrId("whisperfish-settings-auto-unlock-password-field")
                    text: SettingsBridge.plaintext_password
                }
                Button {
                    visible: encryptedDatabase
                    enabled: passwordField.acceptableInput
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: parent.width - 2*Theme.horizontalPageMargin
                    text: passwordField.text.length > 0
                    //: Settings page save autologin password button
                    //% "Save password"
                    ? qsTrId("whisperfish-settings-save-password-button")
                    //: Settings page clear autologin password button
                    //% "clear password"
                    : qsTrId("whisperfish-settings-clear-password-button")
                    onClicked: SettingsBridge.plaintext_password = passwordField.text
                }
                TextArea {
                    visible: encryptedDatabase
                    anchors.horizontalCenter: parent.horizontalCenter
                    readOnly: true
                    width: parent.width
                    font.pixelSize: Theme.fontSizeSmall
                    labelVisible: false
                    //: Settings page info about setting auto unlock password
                    //% "You can enter your password to make Whisperfish unlock the database automatically at startup. Please note that the password is stored in plain text, and as such usage of this feature is not recommended."
                    text: qsTrId("whisperfish-settings-auto-unlock-password-info")
                }
                TextArea {
                    visible: !autostartService.serviceExists
                    anchors.horizontalCenter: parent.horizontalCenter
                    readOnly: true
                    width: parent.width
                    font.pixelSize: Theme.fontSizeSmall
                    labelVisible: false
                    //: Settings page info how to enable autostart manually
                    //% "Whisperfish does not have the permission to change the autostart settings. You can enable or disable autostart manually from the command line by running 'systemctl --user enable harbour-whisperfish.service' or 'systemctl --user disable harbour-whisperfish.service'"
                    text: qsTrId("whisperfish-settings-autostart-manual-info")
                }
                IconTextSwitch {
                    id: enableQuitOnUiClose
                    anchors.horizontalCenter: parent.horizontalCenter
                    //: Settings page enable background mode
                    //% "Background mode"
                    text: qsTrId("whisperfish-settings-enable-background-mode")
                    //: Settings page enable background mode description
                    //% "When enabled, Whisperfish keeps running in the background and can send notifications after the app window has been closed."
                    description: qsTrId("whisperfish-settings-enable-background-mode-description")
                    checked: !SettingsBridge.quit_on_ui_close
                    icon.source: "image://theme/icon-m-levels"
                    icon.rotation: 180
                    onCheckedChanged: {
                        if(checked == SettingsBridge.quit_on_ui_close) {
                            SettingsBridge.quit_on_ui_close = !checked
                            AppState.setMayExit(!checked)
                        }
                    }
                }
                Button {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: parent.width - 2*Theme.horizontalPageMargin
                    enabled: enableQuitOnUiClose.checked
                    //: Settings page quit app button
                    //% "Quit Whisperfish"
                    text: qsTrId("whisperfish-settings-quit-button")
                    onClicked: {
                        Qt.quit()
                    }
                }
            }
            // ------ END BACKGROUND&STARTUP SETTINGS ------

            // ------ BEGIN ADVANCED SETTINGS ------
            SectionHeader {
                //: Settings page advanced section
                //% "Advanced"
                text: qsTrId("whisperfish-settings-advanced-section")
            }
            IconTextSwitch {
                visible: false // XXX: Unimplemented
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page scale image attachments
                //% "Scale JPEG Attachments"
                text: qsTrId("whisperfish-settings-scale-image-attachments")
                //: Settings page scale image attachments description
                //% "Scale down JPEG attachments to save on bandwidth."
                description: qsTrId("whisperfish-settings-scale-image-attachments-description")
                checked: SettingsBridge.scale_image_attachments
                icon.source: "image://theme/icon-m-data-upload"
                onCheckedChanged: {
                    if(checked != SettingsBridge.scale_image_attachments) {
                        SettingsBridge.scale_image_attachments = checked
                    }
                }
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page: debug info toggle
                //% "Debug mode"
                text: qsTrId("whisperfish-settings-debug-mode")
                //: Settings page: debug info toggle extended description
                //% "Show debugging information and controls in the user interface."
                description: qsTrId("whisperfish-settings-debug-mode-description")
                checked: SettingsBridge.debug_mode
                icon.source: "image://theme/icon-m-developer-mode"
                onCheckedChanged: {
                    if(checked != SettingsBridge.debug_mode) {
                        SettingsBridge.debug_mode = checked
                    }
                }
            }
            IconTextSwitch {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Settings page, send verbose logs to systemd journal
                //% "Verbose journal log"
                text: qsTrId("whisperfish-settings-verbose-journal")
                //: Settings page enable verbose logging description
                //% "When enabled, Whisperfish sends verbose output to systemd journal. Requires a restart to take effect."
                description: qsTrId("whisperfish-settings-verbose-journal-description")
                checked: SettingsBridge.verbose
                icon.source: "image://theme/icon-m-about"
                onCheckedChanged: {
                    if(checked != SettingsBridge.verbose) {
                        SettingsBridge.verbose = checked
                    }
                }
            }
            Button {
                visible: SettingsBridge.debug_mode
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2*Theme.horizontalPageMargin
                //: Settings page 'Compact database' button: execute 'VACUUM' command on SQLite-database
                //% "Compact database"
                text: qsTrId("whisperfish-settings-compress-db")
                onClicked: {
                    ClientWorker.compact_db();
                }
            }
            Button {
                visible: SettingsBridge.debug_mode
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2*Theme.horizontalPageMargin
                //: Settings page, test captcha button
                //% "Test captcha"
                text: qsTrId("whisperfish-settings-test-captcha")
                onClicked: {
                    pageStack.push(Qt.resolvedUrl("TestCaptchaPage.qml"));
                }
            }
            // ------ END ADVANCED SETTINGS ------

            // ------ BEGIN STATS ------
            SectionHeader {
                //: Settings page stats section
                //% "Statistics"
                text: qsTrId("whisperfish-settings-stats-section")
            }
            DetailItem {
                //: Settings page websocket status
                //% "Websocket Status"
                label: qsTrId("whisperfish-settings-websocket")
                value: ClientWorker.connected ?
                    //: Settings page connected message
                    //% "Connected"
                    qsTrId("whisperfish-settings-connected") :
                    //: Settings page disconnected message
                    //% "Disconnected"
                    qsTrId("whisperfish-settings-disconnected")
            }
            DetailItem {
                //: Settings page unsent messages
                //% "Unsent Messages"
                label: qsTrId("whisperfish-settings-unsent-messages")
                value: AppState.unsentCount()
            }
            DetailItem {
                //: Settings page total sessions
                //% "Total Sessions"
                label: qsTrId("whisperfish-settings-total-sessions")
                value: AppState.sessionCount()
            }
            DetailItem {
                //: Settings page total messages
                //% "Total Messages"
                label: qsTrId("whisperfish-settings-total-messages")
                value: AppState.messageCount()
            }
            DetailItem {
                //: Settings page total signal contacts
                //% "Signal Contacts"
                label: qsTrId("whisperfish-settings-total-contacts")
                value: AppState.recipientCount()
            }
            DetailItem {
                //: GStreamer version indication in settings
                //% "GStreamer version"
                label: qsTrId("whisperfish-settings-gstreamer-version")
                value: AppState.gstreamer_version
            }
            DetailItem {
                //: Settings page encrypted database
                //% "Encrypted Database"
                label: qsTrId("whisperfish-settings-encrypted-db")
                value: encryptedDatabase ?
                    //: Settings page encrypted db enabled
                    //% "Enabled"
                    qsTrId("whisperfish-settings-encrypted-db-enabled") :
                    //: Settings page encrypted db disabled
                    //% "Disabled"
                    qsTrId("whisperfish-settings-encrypted-db-disabled")
            }
            // ------ END STATS ------
        }
    }
}
