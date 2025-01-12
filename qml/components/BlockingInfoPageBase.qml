import QtQuick 2.2
import Sailfish.Silica 1.0

/*!
  This page shows an icon (Whisperfish by default), a centered
  label with description, custom content, and an optional
  detailed description at the end. All user navigation is blocked.
*/
Page {
    id: root

    property string pageTitle: ""  // optional
    property bool busy: false  // if true: dim the icon and show a spinner

    //: default title of full-screen info pages (below the icon)
    //% "Whisperfish"
    property string mainTitle: qsTrId("whisperfish-info-page-default-title")

    property string mainDescription: ""  // should be set; below the main title
    property string detailedDescription: ""  // optional details below all content
    property bool squashDetails: false  // show details in a smaller font

    property url iconSource: "/usr/share/icons/hicolor/172x172/apps/harbour-whisperfish.png"
    // default a bit larger than BusyIndicatorSize.Large, which is quite large
    // only set this to a smaller value if the derived page will never set busy=true
    property real iconSize: 1.2*Theme.itemSizeExtraLarge

    // add custom content as normal children of the page, e.g. in a column
    default property alias contentItem: infoContentItem.data

    // block any navigation
    backNavigation: false
    forwardNavigation: false
    showNavigationIndicator: false

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: parent.width
            spacing: 1.5*Theme.paddingLarge

            PageHeader {
                title: pageTitle
            }

            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: iconSize
                height: width

                Image {
                    id: appIcon
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectFit
                    // TODO use a higher resolution source image (not SVG though, not supported)
                    source: iconSource
                    verticalAlignment: Image.AlignVCenter
                    opacity: busySpinner.running ? Theme.opacityLow : 1.0
                    Behavior on opacity { FadeAnimator { } }
                }

                BusyIndicator {
                    id: busySpinner
                    anchors.centerIn: parent
                    size: BusyIndicatorSize.Large
                    running: busy
                    opacity: running ? 1.0 : 0.0
                    Behavior on opacity { FadeAnimator { } }
                }
            }

            Column {
                width: parent.width - 4*Theme.horizontalPageMargin
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: Theme.paddingSmall

                Label {
                    width: parent.width
                    text: mainTitle
                    color: Theme.highlightColor
                    font {
                        pixelSize: Theme.fontSizeExtraLarge
                        family: Theme.fontFamilyHeading
                    }
                    horizontalAlignment: Text.AlignHCenter
                }

                Label {
                    width: parent.width
                    text: mainDescription
                    color: Theme.secondaryHighlightColor
                    wrapMode: Text.Wrap
                    // font.pixelSize: Theme.fontSizeMedium
                    font {
                        pixelSize: Theme.fontSizeLarge
                        family: Theme.fontFamilyHeading
                    }
                    horizontalAlignment: Text.AlignHCenter
                }
            }

            Item {
                // spacer: 2*spacing+height
                width: parent.width; height: 1
            }

            Item {
                id: infoContentItem
                width: parent.width
                height: childrenRect.height
            }

            Label {
                width: parent.width - 2*Theme.horizontalPageMargin
                anchors.horizontalCenter: parent.horizontalCenter
                wrapMode: Text.Wrap
                font.pixelSize: squashDetails ? Theme.fontSizeSmall :
                                                Theme.fontSizeMedium
                color: Theme.secondaryHighlightColor
                horizontalAlignment: squashDetails ? Text.AlignLeft :
                                                     Text.AlignHCenter
                text: detailedDescription
            }

            Item {
                // spacer: 2*spacing+height
                width: parent.width; height: 1
            }
        }
    }
}
